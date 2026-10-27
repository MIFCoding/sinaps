<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { Check, Clock, Pill, X } from '@lucide/svelte';
  import {
    api,
    ApiError,
    date,
    eventDateTime,
    localDateTime,
    type AdherenceSummary,
    type AdverseEvent,
    type Patient,
    type Prescription,
    type PrescriptionChange,
  } from './api';
  import type { components } from './generated/api';
  type ManagedDrug = components['schemas']['DrugCatalogVersion'];
  type DiagnosisRecord = components['schemas']['Diagnosis'];

  let {
    patient,
    doctor,
    refreshKey = 0,
    onerror,
    onshowdiagnoses,
  }: {
    patient: Patient;
    doctor: boolean;
    refreshKey?: number;
    onerror?: (cause: unknown) => void;
    onshowdiagnoses?: () => void;
  } = $props();

  let prescriptions = $state<Prescription[]>([]),
    adverseEvents = $state<AdverseEvent[]>([]);
  let medications = $state<ManagedDrug[]>([]), diagnoses = $state<DiagnosisRecord[]>([]);
  let loading = $state(false),
    saving = $state(false),
    error = $state(''),
    diagnosisNeeded = $state(false),
    notice = $state('');
  let editor = $state(false),
    target = $state<Prescription | null>(null),
    drug = $state<Prescription['drug_snapshot'] | ManagedDrug['spec'] | null>(null),
    selectedDiagnosisId = $state(''),
    selectedDrugCode = $state('');
  let draftId = $state(''),
    dosage = $state(''),
    intakeTimes = $state('09:00'),
    startsOn = $state(''),
    endsOn = $state(''),
    instructions = $state('');
  let stale = $state(false),
    history = $state<PrescriptionChange[] | null>(null),
    historyFor = $state('');
  let adherence = $state<Record<string, AdherenceSummary>>({});
  let skipTarget = $state<{ prescription: Prescription; time: string } | null>(null),
    skipReason = $state('');
  let adverseEditor = $state(false),
    adverseId = $state(''),
    adversePrescription = $state(''),
    adverseOccurredAt = $state(''),
    adverseSeverity = $state('moderate'),
    adverseEffectItems = $state<string[]>([]),
    adverseNote = $state('');
  let adversePrescriptionRecord = $derived(
    prescriptions.find((item) => item.id === adversePrescription) ?? null,
  );
  let selectedDiagnosis = $derived(
    diagnoses.find((item) => item.id === selectedDiagnosisId) ?? null,
  );
  let eligibleMedications = $derived(
    medications.filter((item) =>
      selectedDiagnosis && item.spec.diagnosis_codes.includes(selectedDiagnosis.code),
    ),
  );
  let reviewTarget = $state<AdverseEvent | null>(null),
    reviewNote = $state('');
  let alive = false,
    epoch = 0;

  const today = () => {
    const now = new Date();
    return new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  };
  let calendarDate = $state(today());
  const previousDate = (days: number) => {
    const value = new Date();
    value.setDate(value.getDate() - days);
    return new Date(value.getTime() - value.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  };
  const statusLabel = (status: string) =>
    ({ draft: 'Черновик', active: 'Действует', cancelled: 'Отменено', completed: 'Завершено' })[
      status
    ] ?? status;
  const severityLabel = (severity: string) =>
    ({ mild: 'Лёгкая', moderate: 'Средняя', severe: 'Тяжёлая' })[severity] ?? severity;

  function fail(cause: unknown) {
    error = cause instanceof Error ? cause.message : 'Не удалось получить сведения о терапии';
    onerror?.(cause);
  }

  async function load(id: string, token: number) {
    loading = true;
    error = '';
    try {
      const [nextPrescriptions, nextEvents, nextMedications, diagnosisPage] = await Promise.all([
        api('list_prescriptions', { params: { id } }),
        api('list_adverse_events', { params: { id } }),
        doctor ? api('clinical_drugs') : Promise.resolve([]),
        doctor
          ? api('patient_diagnoses', { params: { id }, query: { limit: 50 } })
          : Promise.resolve({ items: [], next_cursor: null }),
      ]);
      if (!alive || token !== epoch) return;
      prescriptions = nextPrescriptions;
      adverseEvents = nextEvents;
      medications = nextMedications;
      diagnoses = diagnosisPage.items;
      const visible = nextPrescriptions.filter((item) => item.status !== 'draft');
      const summaries = await Promise.all(
        visible.map(
          async (item) =>
            [
              item.id,
              await api('prescription_adherence', {
                params: { id, prescription_id: item.id },
                query: { from: previousDate(29), to: today() },
              }),
            ] as const,
        ),
      );
      if (alive && token === epoch) adherence = Object.fromEntries(summaries);
    } catch (cause) {
      if (alive && token === epoch) fail(cause);
    } finally {
      if (alive && token === epoch) loading = false;
    }
  }

  onMount(() => {
    alive = true;
    return () => {
      alive = false;
      ++epoch;
    };
  });

  $effect(() => {
    const id = patient.id,
      refresh = refreshKey,
      token = ++epoch;
    void refresh;
    untrack(() => void load(id, token));
  });

  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!saving) closeDialogs();
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

  function closeDialogs() {
    editor = false;
    skipTarget = null;
    adverseEditor = false;
    reviewTarget = null;
  }

  async function openEditor(value: Prescription | null = null) {
    target = value;
    draftId = value?.id ?? crypto.randomUUID();
    dosage = value?.dosage ?? '';
    intakeTimes = value?.intake_times.join(', ') ?? '09:00';
    startsOn = value?.starts_on ?? today();
    endsOn = value?.ends_on ?? '';
    instructions = value?.instructions ?? '';
    drug = value?.drug_snapshot ?? drug;
    selectedDiagnosisId = value?.diagnosis_id ?? '';
    selectedDrugCode = value?.drug_code ?? '';
    stale = false;
    error = '';
    diagnosisNeeded = false;
    if (!value) {
      saving = true;
      try {
        const [catalog, page] = await Promise.all([
          api('clinical_drugs'),
          api('patient_diagnoses', { params: { id: patient.id }, query: { limit: 50 } }),
        ]);
        medications = catalog;
        diagnoses = page.items;
        if (!diagnoses.some((item) => item.status === 'active')) {
          diagnosisNeeded = true;
          return;
        }
        selectedDiagnosisId = '';
        selectedDrugCode = '';
        drug = null;
      } catch (cause) {
        fail(cause);
        return;
      } finally {
        saving = false;
      }
    }
    editor = true;
  }

  async function savePrescription(event: SubmitEvent) {
    event.preventDefault();
    if (!drug || saving || !selectedDiagnosisId || !selectedDrugCode) return;
    saving = true;
    error = '';
    const body = {
      id: target?.id ?? draftId,
      version: target?.version ?? 1,
      diagnosis_id: selectedDiagnosisId,
      drug_code: selectedDrugCode,
      source_sha256: drug.source_sha256,
      dosage,
      intake_times: intakeTimes
        .split(',')
        .map((value) => value.trim())
        .filter(Boolean),
      starts_on: startsOn,
      ends_on: endsOn || null,
      instructions,
    };
    try {
      if (target)
        await api('update_prescription', {
          params: { id: patient.id, prescription_id: target.id },
          body,
        });
      else await api('create_prescription', { params: { id: patient.id }, body });
      editor = false;
      notice = 'Черновик назначения сохранён. Пациент увидит его только после публикации.';
      await load(patient.id, epoch);
    } catch (cause) {
      stale = cause instanceof ApiError && cause.code === 'stale_version';
      fail(cause);
    } finally {
      saving = false;
    }
  }

  async function transition(value: Prescription, action: 'publish' | 'cancel') {
    if (saving) return;
    saving = true;
    error = '';
    try {
      await api(action === 'publish' ? 'publish_prescription' : 'cancel_prescription', {
        params: { id: patient.id, prescription_id: value.id },
        body: { version: value.version },
      });
      notice = action === 'publish' ? 'Назначение опубликовано пациенту.' : 'Назначение отменено.';
      await load(patient.id, epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  async function loadHistory(value: Prescription) {
    saving = true;
    try {
      history = await api('prescription_changes', {
        params: { id: patient.id, prescription_id: value.id },
      });
      historyFor = value.id;
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  function markFor(value: Prescription, time: string) {
    return adherence[value.id]?.marks.find(
      (mark) => mark.scheduled_on === calendarDate && mark.scheduled_time === time,
    );
  }

  function localDate(value: Date) {
    return new Date(value.getTime() - value.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  }

  function shiftDate(value: string, days: number) {
    const dateValue = new Date(`${value}T12:00:00`);
    dateValue.setDate(dateValue.getDate() + days);
    return localDate(dateValue);
  }

  function calendarDays() {
    return Array.from({ length: 7 }, (_, index) => {
      const value = shiftDate(today(), index);
      return {
        value,
        day: new Intl.DateTimeFormat('ru-RU', { weekday: 'short' }).format(
          new Date(`${value}T12:00:00`),
        ),
        number: new Intl.DateTimeFormat('ru-RU', { day: 'numeric' }).format(
          new Date(`${value}T12:00:00`),
        ),
      };
    });
  }

  function isPastOrToday(value: string) {
    return value <= today();
  }

  async function mark(value: Prescription, time: string, status: 'taken' | 'skipped', reason = '') {
    if (saving) return;
    saving = true;
    error = '';
    try {
      await api('record_adherence', {
        params: { id: patient.id, prescription_id: value.id },
        body: {
          id: crypto.randomUUID(),
          scheduled_on: calendarDate,
          scheduled_time: time,
          status,
          reason,
        },
      });
      skipTarget = null;
      notice = status === 'taken' ? 'Приём отмечен.' : 'Пропуск сохранён.';
      await load(patient.id, epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  function openAdverse() {
    if (!doctor) return;
    adverseId = crypto.randomUUID();
    adversePrescription = prescriptions.find((item) => item.status === 'active')?.id
      ?? prescriptions.find((item) => item.status !== 'draft')?.id ?? '';
    adverseOccurredAt = localDateTime();
    adverseSeverity = 'moderate';
    adverseEffectItems = [];
    adverseNote = '';
    error = '';
    adverseEditor = true;
  }

  async function saveAdverse(event: SubmitEvent) {
    event.preventDefault();
    if (saving) return;
    saving = true;
    try {
      await api('record_adverse_effects', {
        params: { id: patient.id },
        body: {
          id: adverseId,
          prescription_id: adversePrescription,
          occurred_at: eventDateTime(adverseOccurredAt),
          severity: adverseSeverity,
          effect_items: adverseEffectItems,
          note: adverseNote,
        },
      });
      adverseEditor = false;
      notice = 'Выявленные реакции сохранены в истории пациента.';
      await load(patient.id, epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  async function reviewAdverse(event: SubmitEvent) {
    event.preventDefault();
    if (!reviewTarget || saving) return;
    saving = true;
    try {
      await api('review_adverse_event', {
        params: { id: patient.id, event_id: reviewTarget.id },
        body: { version: reviewTarget.version, review_note: reviewNote },
      });
      reviewTarget = null;
      notice = 'Разбор нежелательного явления сохранён.';
      await load(patient.id, epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }
</script>

<section class="card therapy-panel" aria-label="Терапия и приверженность">
  <div class="section-heading">
    <span class="medical-symbol"><Pill size={22} /></span>
    <div>
      <h2>{doctor ? 'Назначения и приверженность' : 'Текущая терапия'}</h2>
      <p class="muted">
        {doctor
          ? 'Решение о назначении, изменении и отмене принимает врач'
          : 'Схема, опубликованная лечащим врачом'}
      </p>
    </div>
    {#if doctor}<div class="therapy-header-actions">
        {#if onshowdiagnoses}<button class="secondary" type="button" onclick={onshowdiagnoses}>Диагнозы</button>{/if}
        <button class="primary" disabled={saving} onclick={() => openEditor()}>Новое назначение</button>
      </div>{/if}
  </div>
  {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
  {#if diagnosisNeeded}<div class="feedback error therapy-diagnosis-prompt" role="alert">
      <span>Чтобы выписать препарат, сначала добавьте пациенту активный диагноз.</span>
      {#if onshowdiagnoses}<button class="secondary" type="button" onclick={onshowdiagnoses}>Перейти к диагнозам</button>{/if}
    </div>{/if}
  {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
  {#if loading}<p class="muted" role="status">Загружаем лечение</p>{/if}

  {#each prescriptions as item (item.id)}
    <article class="therapy-item" id={`therapy-${item.id}`} data-prescription-id={item.id}>
      <div class="section-heading">
        <div>
          <span class="eyebrow">{item.drug_snapshot.ingredient}</span>
          <h3>{item.drug_snapshot.brand}</h3>
        </div>
        <span class="status therapy-status-{item.status}">{statusLabel(item.status)}</span>
      </div>
      <dl class="therapy-details">
        <div>
          <dt>Дозировка</dt>
          <dd>{item.dosage}</dd>
        </div>
        <div>
          <dt>Время приёма</dt>
          <dd>{item.intake_times.join(', ')}</dd>
        </div>
        <div>
          <dt>Период</dt>
          <dd>
            {#if item.ends_on}С {date(item.starts_on)} по {date(item.ends_on)}{:else}С {date(item.starts_on)}, без даты окончания{/if}
          </dd>
        </div>
      </dl>
      {#if item.diagnosis_snapshot}<p class="diagnosis-link"><strong>Диагноз:</strong> {item.diagnosis_snapshot.code}: {item.diagnosis_snapshot.title}</p>
      {:else}<p class="muted small-text">Историческое назначение без сохранённой связи с диагнозом.</p>{/if}
      {#if item.instructions}<p>{item.instructions}</p>{/if}
      <p class="muted small-text">
        Врач: {item.author_name}, версия {item.version}, изменено {date(item.updated_at, true)}
      </p>

      {#if !doctor && item.status === 'active'}
        <div class="medication-calendar" aria-label="Календарь приёма таблеток">
          <div class="calendar-heading">
            <div>
              <h4>Календарь приёма</h4>
              <p class="muted small-text">Напоминания по схеме лечащего врача</p>
            </div>
            <span class="calendar-next">
              {calendarDate === today() ? 'Сегодня' : date(calendarDate)}
            </span>
          </div>
          <div class="calendar-days" aria-label="Ближайшие дни">
            {#each calendarDays() as day}
              <button
                class:active={calendarDate === day.value}
                class:today={day.value === today()}
                class="calendar-day"
                aria-label={`${day.day} ${day.number}`}
                onclick={() => (calendarDate = day.value)}
              >
                <small>{day.day}</small><strong>{day.number}</strong>
                <span class="calendar-dot"></span>
              </button>
            {/each}
          </div>
          <div class="today-intakes">
            <h4>{calendarDate === today() ? 'Приёмы сегодня' : `Приёмы ${date(calendarDate)}`}</h4>
            {#each item.intake_times as time}
              {@const saved = markFor(item, time)}
              <div class="intake-row">
                <span class="intake-time"><Clock size={17} />{time}</span>
                {#if saved}
                  <span
                    class="intake-status"
                    class:taken={saved.status === 'taken'}
                    class:skipped={saved.status === 'skipped'}
                  >
                    {saved.status === 'taken' ? 'Принято' : 'Пропущено'}
                  </span>
                {:else if !isPastOrToday(calendarDate)}
                  <span class="intake-reminder">Напоминание</span>
                {:else}
                  <div class="intake-actions">
                    <button
                      class="secondary compact intake-action"
                      disabled={saving}
                      onclick={() => mark(item, time, 'taken')}><Check size={16} />Принято</button
                    >
                    <button
                      class="secondary compact danger-action intake-action"
                      disabled={saving}
                      onclick={() => {
                        skipTarget = { prescription: item, time };
                        skipReason = '';
                      }}><X size={16} />Пропущено</button
                    >
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if adherence[item.id]}
        <div class="adherence-summary">
          <strong>Приверженность за 30 дней: {adherence[item.id].percent ?? 0}%</strong>
          <span
            >Принято: {adherence[item.id].taken}, пропущено: {adherence[item.id].skipped}, без отметки:
            {adherence[item.id].unmarked}</span
          >
          <small class="muted"
            >В расчёт входят запланированные приёмы до текущей даты, окончания или отмены схемы.</small
          >
        </div>
      {/if}

      {#if doctor}
        <div class="therapy-actions">
          <div class="therapy-action-group">
            {#if item.status === 'draft' || item.status === 'active'}<button
                class="secondary"
                disabled={saving}
                onclick={() => openEditor(item)}>Изменить</button
              >{/if}
            {#if item.status === 'draft'}<button
                class="primary"
                disabled={saving}
                onclick={() => transition(item, 'publish')}>Подтвердить и опубликовать</button
              >{/if}
            {#if item.status === 'draft' || item.status === 'active'}<button
                class="secondary danger-action"
                disabled={saving}
                onclick={() => transition(item, 'cancel')}>Отменить назначение</button
              >{/if}
            <button class="secondary" disabled={saving} onclick={() => loadHistory(item)}
              >История назначения</button
            >
          </div>
        </div>
      {/if}
      {#if doctor && historyFor === item.id && history}
        <div class="clinical-audit">
          {#each history as revision}
            <details>
              <summary
                >Версия {revision.version}, действие: {revision.action}, автор: {revision.author_name}</summary
              >
              <p>Дозировка: {revision.snapshot.dosage}. Время приёма: {revision.snapshot.intake_times.join(', ')}</p>
              <p>{revision.snapshot.instructions || 'Без дополнительных инструкций'}</p>
            </details>
          {/each}
        </div>
      {/if}
    </article>
  {:else}
    {#if !loading}<p class="muted">
        {doctor ? 'Назначений пока нет.' : 'Нет опубликованных назначений.'}
      </p>{/if}
  {/each}

  {#if doctor && prescriptions.some((item) => item.status !== 'draft')}<button class="secondary adverse-button" onclick={openAdverse}
      >Зафиксировать выявленные реакции</button
    >{/if}

  <div class="adverse-list">
    <h3>Выявленные реакции</h3>
    {#each adverseEvents as item (item.id)}
      <article class="adverse-item severity-{item.severity}" id={`therapy-${item.id}`}>
        <div class="section-heading">
          <strong>{severityLabel(item.severity)}</strong><span class="status"
            >{item.status === 'reported' ? 'Ожидает оценки врача' : 'Выявлено врачом'}</span
          >
        </div>
        {#if item.effect_items.length > 0}<p><strong>Реакции:</strong> {item.effect_items.join(', ')}</p>{/if}
        <p>{item.description}</p>
        <p class="muted small-text">
          {#if item.status === 'reported'}Сообщил {item.author_name}, {date(item.occurred_at, true)}{:else}Выявил врач {item.author_name}, {date(item.occurred_at, true)}{/if}
        </p>
        {#if item.status === 'reviewed'}<p class="review-note">
            <strong>Комментарий врача:</strong>
            {item.review_note}
          </p>{/if}
        {#if doctor && item.status === 'reported'}<button
            class="primary"
            disabled={saving}
            onclick={() => {
              reviewTarget = item;
              reviewNote = '';
            }}>Рассмотреть сообщение</button
          >{/if}
      </article>
    {:else}<p class="muted">Врач ещё не зафиксировал выявленные реакции.</p>{/each}
  </div>
</section>

{#if editor && doctor}
  <dialog class="modal card therapy-editor" use:dialog aria-labelledby="therapy-editor-title">
    <form onsubmit={savePrescription}>
      <div class="section-heading">
        <div>
          <span class="eyebrow">{patient.name}</span>
          <h2 id="therapy-editor-title">{target ? 'Изменение назначения' : 'Новое назначение'}</h2>
        </div>
        <button type="button" class="secondary" disabled={saving} onclick={closeDialogs}
          >Закрыть</button
        >
      </div>
      <p class="feedback warning">
        Система сохраняет решение врача и не проверяет автоматически наличие показаний,
        противопоказаний или индивидуальную дозу.
      </p>
      {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
      {#if stale}<p class="feedback warning">
          Назначение уже изменилось. Закройте форму и загрузите актуальные данные.
        </p>{/if}
      {#if target}
        {#if target.diagnosis_snapshot}<p><strong>Диагноз:</strong> {target.diagnosis_snapshot.code}: {target.diagnosis_snapshot.title}</p>
        {:else}<p class="feedback warning">У старого назначения нет сохранённого диагноза. Его нельзя изменить; создайте новую схему после фиксации диагноза.</p>{/if}
        <p><strong>Препарат:</strong> {target.drug_snapshot.brand}, действующее вещество: {target.drug_snapshot.ingredient}</p>
      {:else}
        <label>Сначала диагноз<select bind:value={selectedDiagnosisId} required onchange={() => { selectedDrugCode = ''; drug = null; }}>
          <option value="">Выберите активный диагноз</option>
          {#each diagnoses.filter((item) => item.status === 'active') as diagnosis (diagnosis.id)}
            <option value={diagnosis.id}>{diagnosis.code}: {diagnosis.title}</option>
          {/each}
        </select></label>
        {#if selectedDiagnosisId}
          <label>Подходящий препарат<select bind:value={selectedDrugCode} required onchange={() => {
            drug = eligibleMedications.find((item) => item.drug_code === selectedDrugCode)?.spec ?? null;
          }}>
            <option value="">Выберите препарат</option>
            {#each eligibleMedications as medication (medication.drug_code)}
              <option value={medication.drug_code}>{medication.spec.brand}, {medication.spec.ingredient}</option>
            {/each}
          </select></label>
          {#if !eligibleMedications.length}<p class="muted">Для этого диагноза пока нет опубликованных препаратов.</p>{/if}
        {/if}
      {/if}
      <label
        >Дозировка и схема<input
          bind:value={dosage}
          maxlength="200"
          required
          placeholder="Укажите по инструкции и клиническому решению"
        /></label
      >
      <label
        >Время приёма через запятую<input
          bind:value={intakeTimes}
          required
          placeholder="09:00, 21:00"
        /></label
      >
      <div class="form-row">
        <label>Начало<input type="date" bind:value={startsOn} required /></label><label
          >Окончание<input type="date" bind:value={endsOn} min={startsOn} /></label
        >
      </div>
      <label
        >Инструкции пациенту<textarea bind:value={instructions} maxlength="2000" rows="4"
        ></textarea></label
      >
      <p class="muted">
        Источник: {drug?.source_title}, {drug?.source_date}. После сохранения создаётся скрытый
        черновик.
      </p>
      <button class="primary wide" type="submit" disabled={saving || stale || !drug || !selectedDiagnosisId || !selectedDrugCode || (target !== null && !target.diagnosis_id)}
        >{saving ? 'Сохраняем' : 'Сохранить черновик'}</button
      >
    </form>
  </dialog>
{/if}

{#if skipTarget}
  <dialog class="modal card compact-dialog" use:dialog aria-labelledby="skip-title">
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void mark(skipTarget!.prescription, skipTarget!.time, 'skipped', skipReason);
      }}
    >
      <h2 id="skip-title">Пропуск приёма {skipTarget.time}</h2>
      <label
        >Причина, если хотите сообщить её врачу<textarea
          bind:value={skipReason}
          maxlength="1000"
          rows="3"></textarea></label
      >
      <div class="modal-actions">
        <button type="button" class="secondary" onclick={closeDialogs}>Назад</button><button
          type="submit"
          class="primary"
          disabled={saving}>Сохранить пропуск</button
        >
      </div>
    </form>
  </dialog>
{/if}

{#if adverseEditor && doctor}
  <dialog class="modal card compact-dialog" use:dialog aria-labelledby="adverse-title">
    <form onsubmit={saveAdverse}>
      <h2 id="adverse-title">Фиксация выявленных реакций</h2>
      <p class="muted">Отметьте то, что врач выявил при опросе пациента. Доступны реакции из сохранённой справки препарата.</p>
      <label
        >Назначение<select bind:value={adversePrescription} onchange={() => adverseEffectItems = []} required
          >{#each prescriptions.filter((item) => item.status !== 'draft') as item}<option
              value={item.id}>{item.drug_snapshot.brand}, назначен {date(item.starts_on)}</option
            >{/each}</select
        ></label
      >
      {#if adversePrescriptionRecord}
        {#each adversePrescriptionRecord.drug_snapshot.effects as group}
          <fieldset class="effect-options">
            <legend>{group.frequency}</legend>
            {#each group.items as effect (effect)}
              <label class="effect-option"><input type="checkbox" checked={adverseEffectItems.includes(effect)} onchange={(event) => {
                adverseEffectItems = event.currentTarget.checked
                  ? [...adverseEffectItems, effect]
                  : adverseEffectItems.filter((item) => item !== effect);
              }} />{effect}</label>
            {/each}
          </fieldset>
        {/each}
      {:else}<p class="muted">Нет назначения с опубликованной справкой препарата.</p>{/if}
      <label
        >Когда выявлено<input
          type="datetime-local"
          bind:value={adverseOccurredAt}
          required
        /></label
      >
      <label
        >Выраженность<select bind:value={adverseSeverity}
          ><option value="mild">Лёгкая</option><option value="moderate">Средняя</option><option
            value="severe">Тяжёлая</option
          ></select
        ></label
      >
      <label
        >Комментарий врача, если нужен<textarea bind:value={adverseNote} maxlength="4000" rows="3"
        ></textarea></label
      >
      <div class="modal-actions">
        <button type="button" class="secondary" onclick={closeDialogs}>Отмена</button><button
          class="primary"
          type="submit"
          disabled={saving || !adverseEffectItems.length || !adversePrescription}>Сохранить запись</button
        >
      </div>
    </form>
  </dialog>
{/if}

{#if reviewTarget && doctor}
  <dialog class="modal card compact-dialog" use:dialog aria-labelledby="review-title">
    <form onsubmit={reviewAdverse}>
      <h2 id="review-title">Разбор нежелательного явления</h2>
      <p>{reviewTarget.description}</p>
      <label
        >Комментарий и принятое решение<textarea
          bind:value={reviewNote}
          maxlength="4000"
          rows="5"
          required></textarea></label
      >
      <p class="muted">
        Этот комментарий будет виден пациенту. Изменение или отмена терапии выполняются отдельно.
      </p>
      <div class="modal-actions">
        <button type="button" class="secondary" onclick={closeDialogs}>Отмена</button><button
          class="primary"
          type="submit"
          disabled={saving}>Сохранить разбор</button
        >
      </div>
    </form>
  </dialog>
{/if}
