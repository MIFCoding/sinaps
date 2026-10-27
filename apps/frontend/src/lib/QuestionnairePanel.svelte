<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { api, ApiError, date, type Patient } from './api';
  import type { components } from './generated/api';
  import QuestionnaireTrend from './QuestionnaireTrend.svelte';
  import CustomQuestionnairePanel from './CustomQuestionnairePanel.svelte';
  type Assignment = components['schemas']['QuestionnaireAssignment'];
  type Definition = components['schemas']['QuestionnaireDefinition'];
  type Change = components['schemas']['QuestionnaireChange'];
  type CatalogForm = components['schemas']['CustomQuestionnaireVersion'];
  let {
    patient,
    doctor,
    refreshKey,
    showHistory = false,
    hideWhenEmpty = false,
    onerror,
  }: {
    patient: Patient;
    doctor: boolean;
    refreshKey: number;
    showHistory?: boolean;
    hideWhenEmpty?: boolean;
    onerror: (cause: unknown) => void;
  } = $props();
  let records = $state<Assignment[]>([]),
    cursor = $state<components['schemas']['QuestionnaireCursor'] | null>(null);
  let template = $state<Definition | null>(null),
    loading = $state(false),
    catalogLoading = $state(false),
    saving = $state(false);
  let catalog = $state<CatalogForm[]>([]),
    assignedRefreshKey = $state(0),
    questionnaireSearch = $state(''),
    selectedQuestionnaireId = $state('gad7');
  let error = $state(''),
    notice = $state(''),
    stale = $state(false);
  let mode = $state<'fill' | null>(null),
    target = $state<Assignment | null>(null);
  let answers = $state<(number | null)[]>([]),
    note = $state(''),
    requestId = '';
  let revisions = $state<Change[]>([]),
    auditFor = $state(''),
    auditBusy = $state(false);
  let listRequest = 0;
  let epoch = 0,
    alive = true;
  let answered = $derived(answers.filter((value) => value !== null).length);
  let questionnaireChoices = $derived.by(() => {
    const choices = [
      ...(template ? [{ id: 'gad7', title: template.title }] : []),
      ...catalog
        .filter((form) => form.draft_id !== '3ab28556-f1b4-5d19-ae35-4bd7777260ac')
        .map((form) => ({ id: form.id, title: form.title })),
    ];
    const search = questionnaireSearch.trim().toLocaleLowerCase('ru');
    return choices.filter((item) => item.title.toLocaleLowerCase('ru').includes(search));
  });
  let selectedCatalogForm = $derived(catalog.find((form) => form.id === selectedQuestionnaireId) ?? null);
  let selectedQuestionnaireHint = $derived.by(() => {
    if (selectedQuestionnaireId === 'gad7') {
      return 'Скрининг тревожных симптомов за последние 2 недели. Для взрослых от 18 лет.';
    }
    if (selectedCatalogForm?.draft_id === '92d53473-7d4c-4fc6-9576-c9f32a0b4d09') {
      return 'Скрининг симптомов депрессии за последние 2 недели. Для взрослых от 18 лет.';
    }
    if (selectedCatalogForm) {
      return `${selectedCatalogForm.spec.instruction} Для взрослых от ${selectedCatalogForm.spec.minimum_age} лет.`;
    }
    return '';
  });
  const statuses = { assigned: 'Ожидает заполнения', completed: 'Завершён', cancelled: 'Отменён' };
  const bands = {
    minimal: 'Минимальная выраженность',
    mild: 'Лёгкая выраженность',
    moderate: 'Умеренная выраженность',
    severe: 'Выраженные симптомы',
  };
  function fail(cause: unknown) {
    error = cause instanceof Error ? cause.message : 'Не удалось загрузить опросники';
    stale = cause instanceof ApiError && cause.status === 409;
    if (cause instanceof ApiError && cause.status === 401) onerror(cause);
  }
  async function load(id: string, token: number, more = false) {
    if (more && (!cursor || loading)) return;
    loading = true;
    error = '';
    const request = ++listRequest;
    try {
      const result = await api('list_questionnaires', {
        params: { id },
        query: { ...(more && cursor ? cursor : {}) },
      });
      if (!alive || token !== epoch || request !== listRequest) return;
      records = more
        ? [...records, ...result.items.filter((row) => !records.some((old) => old.id === row.id))]
        : result.items;
      cursor = result.next_cursor;
    } catch (cause) {
      if (alive && token === epoch && request === listRequest) fail(cause);
    } finally {
      if (alive && token === epoch && request === listRequest) loading = false;
    }
  }
  async function loadTemplate(token: number) {
    try {
      const result = await api('questionnaire_definition');
      if (alive && token === epoch) template = result;
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    }
  }
  async function loadCatalog(token: number) {
    catalogLoading = true;
    try {
      const result = await api('custom_questionnaire_catalog');
      if (alive && token === epoch) catalog = result;
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) catalogLoading = false;
    }
  }
  function reload() {
    const token = ++epoch;
    mode = null;
    target = null;
    template = null;
    catalog = [];
    catalogLoading = false;
    error = '';
    notice = '';
    stale = false;
    records = [];
    cursor = null;
    revisions = [];
    auditFor = '';
    saving = false;
    auditBusy = false;
    void load(patient.id, token);
    if (doctor) {
      void loadTemplate(token);
      void loadCatalog(token);
    }
  }
  $effect(() => {
    patient.id;
    doctor;
    refreshKey;
    showHistory;
    untrack(reload);
  });
  onDestroy(() => {
    alive = false;
    epoch++;
  });
  function openFill(row: Assignment) {
    target = row;
    answers = row.definition.questions.map(() => null);
    error = '';
    stale = false;
    mode = 'fill';
  }
  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!saving) mode = null;
    };
    node.addEventListener('cancel', cancel);
    return {
      destroy() {
        node.removeEventListener('cancel', cancel);
        node.close();
        previous?.focus();
      },
    };
  }
  function replace(row: Assignment) {
    records = [row, ...records.filter((old) => old.id !== row.id)].sort(
      (a, b) => Date.parse(b.created_at) - Date.parse(a.created_at) || b.id.localeCompare(a.id),
    );
  }
  async function assignQuestionnaire(event: SubmitEvent) {
    event.preventDefault();
    if (saving || stale || !questionnaireChoices.some((item) => item.id === selectedQuestionnaireId)) return;
    const token = epoch,
      id = patient.id;
    saving = true;
    error = '';
    try {
      requestId = crypto.randomUUID();
      if (selectedQuestionnaireId === 'gad7' && template) {
        const result = await api('assign_questionnaire', {
          params: { id },
          body: { id: requestId, scale_version: template.version, note },
        });
        if (alive && token === epoch) {
          replace(result);
          note = '';
          notice = 'Опросник назначен пациенту';
          void load(id, token);
        }
      } else if (selectedCatalogForm) {
        await api('assign_custom_questionnaire', {
          params: { id },
          body: { id: requestId, form_id: selectedCatalogForm.id, note },
        });
        if (alive && token === epoch) {
          notice = `${selectedCatalogForm.title} назначен пациенту`;
          note = '';
          assignedRefreshKey++;
          return;
        }
      }
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) saving = false;
    }
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (saving || stale || !target || answered !== target.definition.questions.length) return;
    const token = epoch,
      id = patient.id;
    saving = true;
    error = '';
    try {
      const result = await api('submit_questionnaire', {
        params: { id, assignment_id: target.id },
        body: {
          version: target.version,
          scale_version: target.scale_version,
          answers: answers as number[],
        },
      });
      if (alive && token === epoch) {
        replace(result);
        mode = null;
        notice = 'Ответы и результат сохранены';
        void load(id, token);
      }
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) saving = false;
    }
  }
  async function cancel(row: Assignment) {
    if (saving) return;
    const token = epoch,
      id = patient.id;
    saving = true;
    error = '';
    notice = '';
    try {
      const result = await api('cancel_questionnaire', {
        params: { id, assignment_id: row.id },
        body: { version: row.version },
      });
      if (alive && token === epoch) {
        replace(result);
        notice = 'Назначение отменено';
      }
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) saving = false;
    }
  }
  async function audit(row: Assignment) {
    if (auditBusy) return;
    const token = epoch;
    auditBusy = true;
    try {
      const result = await api('questionnaire_changes', {
        params: { id: patient.id, assignment_id: row.id },
      });
      if (alive && token === epoch) {
        revisions = result;
        auditFor = row.id;
      }
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) auditBusy = false;
    }
  }
</script>

{#snippet source(value: Definition)}
  <details class="questionnaire-source">
    <summary>Версия и источник опросника</summary>
    <p>{value.version}</p>
    <p>{value.attribution}</p>
    <p>{value.limitations}</p>
    <p>
      <a href={value.source_url} target="_blank" rel="noreferrer">Русскоязычная адаптация</a>,
      <a href={value.scoring_url} target="_blank" rel="noreferrer">Правила подсчёта</a>
    </p>
    <a href={value.license_url} target="_blank" rel="noreferrer">{value.license}</a>
  </details>
{/snippet}

{#if !hideWhenEmpty || records.length || loading || error}
  <section class="card questionnaires" aria-label="Опросники пациента">
    <div class="section-heading">
      <div>
        <h2>Опросники</h2>
        <p class="muted">Оценка симптомов по назначению врача</p>
      </div>
    </div>
    {#if doctor}<form class="questionnaire-assign-form" onsubmit={assignQuestionnaire}>
        <label for="questionnaire-search">Поиск опросника</label>
        <input id="questionnaire-search" type="search" bind:value={questionnaireSearch} placeholder="Введите название" />
        <label for="questionnaire-choice">Опросник</label>
        <select id="questionnaire-choice" bind:value={selectedQuestionnaireId} disabled={catalogLoading || saving}>
          {#each questionnaireChoices as choice (choice.id)}
            <option value={choice.id}>{choice.title}</option>
          {:else}<option value="">Опросники не найдены</option>{/each}
        </select>
        {#if selectedQuestionnaireHint}<p class="muted small-text questionnaire-selection-hint">{selectedQuestionnaireHint}</p>{/if}
        <label for="questionnaire-note">Комментарий пациенту</label>
        <textarea id="questionnaire-note" rows="3" maxlength="2000" bind:value={note}></textarea>
        <button class="primary" type="submit" disabled={saving || catalogLoading || !questionnaireChoices.some((item) => item.id === selectedQuestionnaireId)}>
          {saving ? 'Сохраняем' : 'Назначить'}
        </button>
      </form>{/if}
    {#if error && !mode}<div class="feedback error" role="alert">
        {error}<button class="text-link" onclick={reload}>Обновить опросники</button>
      </div>{/if}
    {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
    {#if showHistory}
      {#each [...new Set(records
            .filter((row) => row.status === 'completed')
            .map((row) => row.scale_version))] as version}
        <QuestionnaireTrend records={records.filter((row) => row.scale_version === version)} />
      {/each}
    {/if}
    {#each records as row (row.id)}
      <article class="questionnaire-record" data-assignment-id={row.id}>
        <div class="section-heading">
          <div>
            <h3>{row.definition.title}</h3>
            <p class="muted">{statuses[row.status]}. Назначено {date(row.created_at, true)}</p>
          </div>
        </div>
        <p class="muted">Назначил: {row.author_name}</p>
        {#if row.note}<p class="questionnaire-note">{row.note}</p>{/if}
        {#if row.status === 'assigned'}
          {#if doctor}<button class="secondary" disabled={saving} onclick={() => cancel(row)}
              >Отменить назначение</button
            >
          {:else}<button class="primary" onclick={() => openFill(row)}>Заполнить {row.definition.title}</button>{/if}
        {:else if row.status === 'completed'}
          <div class="questionnaire-result-card">
            <div class="questionnaire-result-summary">
              <strong class="questionnaire-result-score" aria-label={`${row.score} из 21 балла`}>
                {row.score}<span>/ 21</span>
              </strong>
              <div>
                <span class="questionnaire-result-label">Результат GAD‑7</span>
                <span class="questionnaire-band"
                  >{row.band ? bands[row.band] : 'Результат сохранён'}</span
                >
              </div>
            </div>
            <div
              class="questionnaire-score-meter"
              role="meter"
              aria-label="Баллы GAD-7"
              aria-valuemin="0"
              aria-valuemax="21"
              aria-valuenow={row.score ?? 0}
            >
              <span style:width={`${((row.score ?? 0) / 21) * 100}%`}></span>
            </div>
          </div>
          <p class="muted">
            Заполнено {row.completed_at ? date(row.completed_at, true) : ''}. Скрининговый
            результат, не диагноз.
          </p>
          <details>
            <summary>Сохранённые ответы</summary>
            <ol class="questionnaire-answers">
              {#each row.definition.questions as question, index}<li>
                  {question}<strong
                    >{row.answers ? row.definition.options[row.answers[index]] : ''}</strong
                  >
                </li>{/each}
            </ol>
          </details>
        {:else}<p class="muted">
            Отменено {row.cancelled_at ? date(row.cancelled_at, true) : ''}
          </p>{/if}
        {@render source(row.definition)}
        {#if doctor}<button class="text-link" disabled={auditBusy} onclick={() => audit(row)}
            >Журнал опросника</button
          >{/if}
        {#if doctor && auditFor === row.id}<div class="questionnaire-audit">
            {#each revisions as revision}<details>
                <summary>Версия {revision.version}, {revision.author_name}</summary>
                <p>
                  {date(revision.created_at, true)}. {statuses[
                    revision.action as keyof typeof statuses
                  ]}
                </p>
                {#if revision.snapshot.score !== null}<p>{revision.snapshot.score} / 21</p>{/if}
                <p>{revision.snapshot.scale_version}</p>
              </details>{/each}
          </div>{/if}
      </article>
    {:else}{#if !loading}<p class="muted">
          {doctor ? 'Опросники ещё не назначены.' : 'Назначенных опросников пока нет.'}
        </p>{/if}{/each}
    {#if loading}<p class="muted" role="status">Загружаем опросники</p>{/if}
    {#if cursor}<button
        class="secondary"
        disabled={loading}
        onclick={() => load(patient.id, epoch, true)}>Ещё опросники</button
      >{/if}
    {#if records.some((row) => row.status === 'completed') && !showHistory}<p
        class="muted small-text"
      >
        Динамика результатов доступна в разделе «Динамика».
      </p>{/if}
  </section>
{/if}

<CustomQuestionnairePanel {patient} {doctor} embedded={doctor} refreshKey={refreshKey + assignedRefreshKey} {onerror} />

{#if mode === 'fill'}<dialog
    class="modal questionnaire-dialog"
    use:dialog
    aria-labelledby="questionnaire-dialog-title"
  >
    <form onsubmit={save}>
      <div class="section-heading">
        <h2 id="questionnaire-dialog-title">Заполнение: {target?.definition.title ?? ''}</h2>
        <button
          class="icon-button"
          type="button"
          aria-label="Закрыть опросник"
          disabled={saving}
          onclick={() => (mode = null)}>×</button
        >
      </div>
      {#if target}
        <p>{target.definition.instruction}</p>
        <p class="muted">Ответы будут доступны вашему врачу.</p>
        <div class="questionnaire-progress-wrap">
          <div
            class="questionnaire-progress"
            role="progressbar"
            aria-label="Заполнение опросника"
            aria-valuemin="0"
            aria-valuemax={target.definition.questions.length}
            aria-valuenow={answered}
          >
            <span style:width={`${(answered / target.definition.questions.length) * 100}%`}></span>
          </div>
          <p class="muted">
            Отвечено: <strong>{answered}</strong> из {target.definition.questions.length}
          </p>
        </div>
        {#each target.definition.questions as question, index}
          <fieldset class="questionnaire-question">
            <legend>{index + 1}. {question}</legend>
            {#each target.definition.options as label, value}<label
                ><input
                  type="radio"
                  name={`question-${index}`}
                  {value}
                  bind:group={answers[index]}
                  required
                  disabled={saving}
                /><span>{label}</span></label
              >{/each}
          </fieldset>
        {/each}
        {@render source(target.definition)}
      {/if}
      {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
      {#if stale}<button class="secondary" type="button" onclick={reload}
          >Загрузить актуальное назначение</button
        >{/if}
      <div class="modal-actions">
        <button class="secondary" type="button" disabled={saving} onclick={() => (mode = null)}
          >Закрыть</button
        >
        <button
          class="primary"
          type="submit"
          disabled={saving || stale || answered !== target?.definition.questions.length}
          >{saving ? 'Сохраняем ответы' : 'Отправить ответы'}</button
        >
      </div>
    </form>
  </dialog>{/if}
