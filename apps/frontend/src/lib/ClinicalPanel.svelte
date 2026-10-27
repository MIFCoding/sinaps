<script lang="ts">
  import { onDestroy } from 'svelte';
  import { api, ApiError, date, type Patient } from './api';
  import type { components } from './generated/api';
  type Diagnosis = components['schemas']['Diagnosis'];
  type Finding = components['schemas']['SelectedFinding'];
  type CatalogFinding = components['schemas']['CatalogFinding'];
  type CatalogCode = components['schemas']['CatalogDiagnosis'];
  type Audit = components['schemas']['DiagnosisAudit'];
  let {
    patient,
    doctor,
    refreshKey,
    onerror,
  }: { patient: Patient; doctor: boolean; refreshKey: number; onerror: (cause: unknown) => void } =
    $props();
  let records = $state<Diagnosis[]>([]),
    cursor = $state<components['schemas']['DiagnosisCursor'] | null>(null);
  let loading = $state(false),
    error = $state(''),
    notice = $state('');
  let editing = $state(false),
    saving = $state(false),
    stale = $state(false);
  let target = $state<Diagnosis | null>(null),
    selected = $state<Finding[]>([]);
  let code = $state<CatalogCode | null>(null),
    note = $state(''),
    diagnosedOn = $state(''),
    status = $state<'active' | 'resolved'>('active');
  let findingSearch = $state(''),
    codeSearch = $state('');
  let findings = $state<CatalogFinding[]>([]),
    codes = $state<CatalogCode[]>([]);
  let findingOffset = $state<number | null>(null),
    codeOffset = $state<number | null>(null),
    findingTotal = $state(0),
    codeTotal = $state(0);
  let findingBusy = $state(false),
    codeBusy = $state(false),
    catalogVersion = $state('');
  let source = $state<components['schemas']['SourceSection'] | null>(null),
    sourceBusy = $state(false);
  let audit = $state<Audit | null>(null),
    auditFor = $state(''),
    auditBusy = $state(false);
  let alive = true,
    epoch = 0,
    findingRequest = 0,
    codeRequest = 0,
    sourceRequest = 0;
  let draftId = crypto.randomUUID();
  onDestroy(() => {
    alive = false;
    epoch++;
  });
  function fail(cause: unknown) {
    if (cause instanceof ApiError && cause.status === 401) onerror(cause);
    else
      error = cause instanceof Error ? cause.message : 'Не удалось получить клинические сведения';
  }
  async function loadRecords(id: string, token: number, more = false) {
    if (more && (loading || !cursor)) return;
    loading = true;
    error = '';
    try {
      const page = await api('patient_diagnoses', {
        params: { id },
        query: { ...(more && cursor ? cursor : {}) },
      });
      if (!alive || token !== epoch) return;
      records = more
        ? [...records, ...page.items.filter((row) => !records.some((old) => old.id === row.id))]
        : page.items;
      cursor = page.next_cursor;
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) loading = false;
    }
  }
  $effect(() => {
    const id = patient.id;
    void refreshKey;
    const token = ++epoch;
    editing = false;
    selected = [];
    records = [];
    cursor = null;
    audit = null;
    auditBusy = false;
    auditFor = '';
    notice = '';
    ++sourceRequest;
    sourceBusy = false;
    source = null;
    void loadRecords(id, token);
  });
  async function loadFindings(search: string, more = false) {
    if (more && (findingBusy || findingOffset === null)) return;
    const token = ++findingRequest;
    findingBusy = true;
    try {
      const page = await api('clinical_findings', {
        query: { search, offset: more ? (findingOffset ?? 0) : 0, limit: 20 },
      });
      if (!alive || token !== findingRequest) return;
      findings = more ? [...findings, ...page.items] : page.items;
      findingTotal = page.total_count;
      findingOffset = page.next_offset;
      catalogVersion = page.version;
    } catch (cause) {
      if (alive && token === findingRequest) fail(cause);
    } finally {
      if (alive && token === findingRequest) findingBusy = false;
    }
  }
  async function loadCodes(search: string, more = false) {
    if (more && (codeBusy || codeOffset === null)) return;
    const token = ++codeRequest;
    codeBusy = true;
    try {
      const page = await api('clinical_codes', {
        query: { search, offset: more ? (codeOffset ?? 0) : 0, limit: 20 },
      });
      if (!alive || token !== codeRequest) return;
      codes = more ? [...codes, ...page.items] : page.items;
      codeTotal = page.total_count;
      codeOffset = page.next_offset;
      catalogVersion = page.version;
    } catch (cause) {
      if (alive && token === codeRequest) fail(cause);
    } finally {
      if (alive && token === codeRequest) codeBusy = false;
    }
  }
  $effect(() => {
    if (!editing || !doctor) return;
    const search = findingSearch;
    ++findingRequest;
    findingBusy = true;
    findings = [];
    findingOffset = null;
    findingTotal = 0;
    const timer = setTimeout(() => void loadFindings(search), 200);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    if (!editing || !doctor) return;
    const search = codeSearch;
    ++codeRequest;
    codeBusy = true;
    codes = [];
    codeOffset = null;
    codeTotal = 0;
    const timer = setTimeout(() => void loadCodes(search), 200);
    return () => clearTimeout(timer);
  });
  function open(row: Diagnosis | null = null) {
    target = row;
    draftId = crypto.randomUUID();
    selected = row ? [...row.findings] : [];
    code = row ? { code: row.code, title: row.title, source_page: 0, recordable: true } : null;
    note = row?.note ?? '';
    status = row?.status === 'resolved' ? 'resolved' : 'active';
    diagnosedOn =
      row?.diagnosed_on ??
      new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
    findingSearch = '';
    codeSearch = '';
    ++sourceRequest;
    sourceBusy = false;
    source = null;
    stale = false;
    error = '';
    notice = '';
    editing = true;
  }
  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!saving) editing = false;
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
  function toggle(item: CatalogFinding, checked: boolean) {
    selected = checked
      ? [
          ...selected,
          {
            id: item.id,
            label: item.label,
            source_code: item.source_code,
            source_page: item.source_page,
          },
        ]
      : selected.filter((old) => old.id !== item.id);
  }
  async function showSource(value: string) {
    const token = ++sourceRequest;
    sourceBusy = true;
    try {
      const result = await api('clinical_source', { params: { code: value } });
      if (alive && token === sourceRequest) source = result;
    } catch (cause) {
      if (alive && token === sourceRequest) fail(cause);
    } finally {
      if (alive && token === sourceRequest) sourceBusy = false;
    }
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!code || !selected.length || saving || !catalogVersion) return;
    const token = epoch,
      id = patient.id;
    saving = true;
    error = '';
    const body = {
      id: target?.id ?? draftId,
      version: target?.version ?? 1,
      catalog_version: catalogVersion,
      code: code.code,
      finding_ids: selected.map((item) => item.id),
      diagnosed_on: diagnosedOn,
      status,
      note,
    };
    try {
      if (target) await api('update_diagnosis', { params: { id, diagnosis_id: target.id }, body });
      else await api('create_diagnosis', { params: { id }, body });
      if (!alive || token !== epoch) return;
      editing = false;
      audit = null;
      notice = 'Диагноз и выбранные пункты сохранены.';
      await loadRecords(id, token);
    } catch (cause) {
      if (alive && token === epoch) {
        stale = cause instanceof ApiError && cause.code === 'stale_version';
        fail(cause);
      }
    } finally {
      if (alive && token === epoch) saving = false;
    }
  }
  async function reloadTarget() {
    if (!target) return;
    const token = epoch;
    saving = true;
    try {
      const value = await api('patient_diagnosis', {
        params: { id: patient.id, diagnosis_id: target.id },
      });
      if (alive && token === epoch) open(value);
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) saving = false;
    }
  }
  async function loadAudit(row: Diagnosis, more = false) {
    if (auditBusy) return;
    const token = epoch;
    auditBusy = true;
    error = '';
    try {
      const result = await api('diagnosis_changes', {
        params: { id: patient.id, diagnosis_id: row.id },
        query: more && audit?.next_version ? { before_version: audit.next_version } : {},
      });
      if (!alive || token !== epoch) return;
      audit = {
        ...result,
        items:
          more && auditFor === row.id && audit ? [...audit.items, ...result.items] : result.items,
      };
      auditFor = row.id;
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) auditBusy = false;
    }
  }
</script>

<section
  class="card clinical-panel"
  class:patient-diagnoses={!doctor}
  aria-label="Диагнозы и проявления"
>
  <div class="section-heading">
    {#if !doctor}<span class="medical-symbol"><img src="/design/medical.svg" alt="" /></span>{/if}
    <div>
      <h2>{doctor ? 'Диагнозы и отмеченные проявления' : 'Диагноз'}</h2>
      <p class="muted">
        {doctor
          ? 'Диагноз устанавливает врач после клинической оценки'
          : 'Диагнозы и сведения, сохранённые вашим врачом'}
      </p>
    </div>
    {#if doctor}<button class="primary" onclick={() => open()}>Добавить диагноз</button>{/if}
  </div>
  {#if error && !editing}<p class="feedback error" role="alert">{error}</p>{/if}
  {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
  {#each records as row (row.id)}
    <article class="clinical-diagnosis" data-diagnosis-id={row.id}>
      <div class="section-heading">
        <div>
          <span class="eyebrow">МКБ-10: {row.code}</span>
          <h3>{row.title}</h3>
        </div>
        <span class="status">{row.status === 'active' ? 'Действующий' : 'Завершён'}</span>
      </div>
      <p class="muted">Дата диагноза: {date(row.diagnosed_on)}. Врач: {row.author_name}</p>
      <h4>Отмечено врачом</h4>
      <ul class="clinical-selected">
        {#each row.findings as item}<li>{item.label}</li>{/each}
      </ul>
      {#if row.note}<p class="clinical-note">{row.note}</p>{/if}
      <small class="muted">Изменено {date(row.updated_at, true)}. Автор изменения: {row.updated_by_name}</small>
      {#if doctor}<div class="clinical-actions">
          <button class="secondary" onclick={() => open(row)}>Изменить диагноз</button><button
            class="text-link"
            disabled={auditBusy}
            onclick={() => loadAudit(row)}>История диагноза</button
          >
        </div>{/if}
      {#if doctor && audit && auditFor === row.id}<div class="clinical-audit">
          {#each audit.items as revision}<details>
              <summary
                >Версия {revision.version}, {date(revision.created_at, true)}, {revision.author_name}</summary
              >
              <p>{revision.snapshot.code}: {revision.snapshot.title}</p>
              <ul>
                {#each revision.snapshot.findings as item}<li>{item.label}</li>{/each}
              </ul>
              <p>{revision.snapshot.note}</p>
            </details>{/each}
          {#if audit.next_version}<button
              class="secondary"
              disabled={auditBusy}
              onclick={() => loadAudit(row, true)}>Ещё версии диагноза</button
            >{/if}
        </div>{/if}
    </article>
  {:else}{#if !loading}<p class="muted">
        {doctor
          ? 'Диагнозы ещё не внесены. Выберите проявления, затем код диагноза.'
          : 'Врач ещё не добавил диагноз.'}
      </p>{/if}{/each}
  {#if loading}<p role="status">Загружаем диагнозы</p>{/if}
  {#if cursor}<button
      class="secondary"
      disabled={loading}
      onclick={() => loadRecords(patient.id, epoch, true)}>Ещё диагнозы</button
    >{/if}
  {#if error && !records.length && !loading}<button
      class="secondary"
      onclick={() => loadRecords(patient.id, epoch)}>Повторить загрузку диагнозов</button
    >{/if}
</section>

{#if editing && doctor}
  <dialog class="modal card clinical-editor" use:dialog aria-labelledby="clinical-editor-title">
    <form onsubmit={save}>
      <div class="section-heading">
        <div>
          <span class="eyebrow">{patient.name}</span>
          <h2 id="clinical-editor-title">{target ? 'Изменить диагноз' : 'Новый диагноз'}</h2>
        </div>
        <button type="button" class="secondary" disabled={saving} onclick={() => (editing = false)}
          >Закрыть</button
        >
      </div>
      {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
      {#if stale}<button type="button" class="secondary" disabled={saving} onclick={reloadTarget}
          >Загрузить актуальный диагноз</button
        >{/if}
      <p class="muted">
        Исследовательские критерии МКБ-10 и записи, опубликованные администратором. В списке
        есть симптомы и другие диагностические пункты; отметки не вычисляют диагноз.
      </p>
      <div class="clinical-steps">
        <section>
          <h3>1. Отметьте проявления</h3>
          <label
            >Поиск симптомов и критериев<input
              bind:value={findingSearch}
              maxlength="120"
              placeholder="Симптом, раздел или код МКБ-10"
            /></label
          >
          <p class="muted">Найдено пунктов: {findingTotal}, выбрано: {selected.length}</p>
          <div class="clinical-findings">
            {#each findings as item (item.id)}<div class="clinical-finding">
                <label
                  ><input
                    type="checkbox"
                    checked={selected.some((value) => value.id === item.id)}
                    disabled={saving ||
                      (!selected.some((value) => value.id === item.id) && selected.length >= 100)}
                    onchange={(event) => toggle(item, event.currentTarget.checked)}
                  /><span
                    >{item.label}<small>Код: {item.source_code}. {item.id.startsWith('managed:') ? 'Источник, страница ' : 'PDF, страница '}{item.source_page}</small
                    ></span
                  ></label
                >
                <details>
                  <summary>Условия пункта в источнике</summary>
                  <p>{item.context}</p>
                  <button
                    type="button"
                    class="text-link"
                    disabled={sourceBusy}
                    onclick={() => showSource(item.id.startsWith('managed:') ? item.id : item.source_code)}>Показать раздел источника</button
                  >
                </details>
              </div>{/each}
          </div>
          {#if findingBusy}<p role="status">Загружаем список</p>{/if}
          {#if findingOffset !== null}<button
              type="button"
              class="secondary"
              disabled={findingBusy}
              onclick={() => loadFindings(findingSearch, true)}>Ещё пункты справочника</button
            >{/if}
          {#if selected.length}<details class="clinical-selection">
              <summary>Выбрано: {selected.length}</summary>{#each selected as item}<div>
                  <span>{item.label}</span><button
                    type="button"
                    class="text-link"
                    disabled={saving}
                    aria-label={`Убрать: ${item.label}`}
                    onclick={() => (selected = selected.filter((value) => value.id !== item.id))}
                    >Убрать</button
                  >
                </div>{/each}
            </details>{/if}
        </section>
        <section>
          <h3>2. Выберите диагноз</h3>
          <label
            >Поиск диагноза<input
              bind:value={codeSearch}
              maxlength="120"
              placeholder="Код или название"
              disabled={!selected.length || saving}
            /></label
          >
          {#if code}<p class="clinical-code-chosen">
              <strong>{code.code}: {code.title}</strong>
            </p>{/if}
          {#if selected.length}<p class="muted">Найдено кодов: {codeTotal}</p>
            <div class="clinical-codes">
              {#each codes as item (item.code)}<button
                  type="button"
                  class:chosen={code?.code === item.code}
                  disabled={saving}
                  aria-pressed={code?.code === item.code}
                  onclick={() => {
                    code = item;
                    source = null;
                    ++sourceRequest;
                    sourceBusy = false;
                  }}><strong>{item.code}</strong><span>{item.title}</span></button
                >{/each}
            </div>
            {#if codeBusy}<p role="status">Загружаем диагнозы</p>{/if}
            {#if codeOffset !== null}<button
                type="button"
                class="secondary"
                disabled={codeBusy}
                onclick={() => loadCodes(codeSearch, true)}>Ещё коды диагнозов</button
              >{/if}
          {:else}<p class="muted">Сначала отметьте проявления в списке.</p>{/if}
          {#if code}<button
              type="button"
              class="text-link"
              disabled={sourceBusy}
              onclick={() => showSource(code!.code)}>Сведения источника выбранного кода</button
            >{/if}
          <label
            >Дата диагноза<input
              type="date"
              bind:value={diagnosedOn}
              min={patient.birth_date}
              required
            /></label
          >
          <label
            >Состояние диагноза<select bind:value={status}
              ><option value="active">Действующий</option><option value="resolved">Завершён</option
              ></select
            ></label
          >
          <label
            >Комментарий, видимый пациенту<textarea bind:value={note} maxlength="4000" rows="3"
            ></textarea></label
          >
        </section>
      </div>
      {#if source}<details open class="clinical-source">
          <summary>Раздел {source.code}, страница {source.source_page}</summary>
          <p>{source.text}</p>
        </details>{/if}
      <div class="clinical-editor-footer">
        <span>Выбрано пунктов: {selected.length}</span><button
          type="submit"
          class="primary"
          disabled={saving || stale || !selected.length || !code || !catalogVersion}
          >{saving ? 'Сохраняем' : 'Сохранить диагноз'}</button
        >
      </div>
    </form>
  </dialog>
{/if}
