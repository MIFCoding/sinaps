<script lang="ts">
  import { CalendarDays, ClipboardList, HeartPulse, Moon, UserRound } from '@lucide/svelte';
  import AlertPanel from './AlertPanel.svelte';
  import AppointmentsPanel from './AppointmentsPanel.svelte';
  import ClinicalPanel from './ClinicalPanel.svelte';
  import ClinicalRecommendationsPanel from './ClinicalRecommendationsPanel.svelte';
  import EventTimeline from './EventTimeline.svelte';
  import MedicalHistory from './MedicalHistory.svelte';
  import MetricChart from './MetricChart.svelte';
  import ProfileHistory from './ProfileHistory.svelte';
  import QuestionnairePanel from './QuestionnairePanel.svelte';
  import TherapyPanel from './TherapyPanel.svelte';
  import {
    date,
    eventKinds,
    type ChangesPage,
    type DiaryEntry,
    type EventKind,
    type HistoryCursor,
    type ObservationEvent,
    type Patient,
    type ProfileChange,
  } from './api';
  import Avatar from './Avatar.svelte';

  type PatientTab = 'overview' | 'observation' | 'diagnoses' | 'treatment' | 'history';
  const tabs: { id: PatientTab; label: string }[] = [
    { id: 'overview', label: 'Обзор' },
    { id: 'observation', label: 'Наблюдение' },
    { id: 'diagnoses', label: 'Диагнозы' },
    { id: 'treatment', label: 'Лечение' },
    { id: 'history', label: 'История' },
  ];
  let {
    patient,
    refreshKey,
    entries,
    diaryCursor,
    diaryMoreBusy,
    events,
    eventCursor,
    eventMoreBusy,
    eventPageFailed,
    changes,
    changesCursor,
    changesMoreBusy,
    loading,
    tab = $bindable<PatientTab>('overview'),
    eventFilter = $bindable<EventKind | 'all'>('all'),
    onedit,
    oninvite,
    onerror,
    onloadDiary,
    onloadEvents,
    onretryEvents,
    onloadChanges,
  }: {
    patient: Patient;
    refreshKey: number;
    entries: DiaryEntry[];
    diaryCursor: HistoryCursor | null;
    diaryMoreBusy: boolean;
    events: ObservationEvent[];
    eventCursor: HistoryCursor | null;
    eventMoreBusy: boolean;
    eventPageFailed: boolean;
    changes: ProfileChange[];
    changesCursor: ChangesPage['next_cursor'];
    changesMoreBusy: boolean;
    loading: boolean;
    tab?: PatientTab;
    eventFilter?: EventKind | 'all';
    onedit: () => void;
    oninvite: () => void;
    onerror: (cause: unknown) => void;
    onloadDiary: () => Promise<void>;
    onloadEvents: () => Promise<void>;
    onretryEvents: () => Promise<void>;
    onloadChanges: () => Promise<void>;
  } = $props();

  let period = $state(30);
  let periodEntries = $derived(
    entries.filter((entry) => {
      const measuredAt = Date.parse(entry.measured_at);
      const now = Date.now();
      return measuredAt >= now - period * 86400000 && measuredAt <= now;
    }),
  );
  function selectTab(value: PatientTab) {
    tab = value;
  }
</script>

<section class="doctor-patient-workspace">
  <div class="patient-workspace-tabs" aria-label="Разделы карточки пациента" role="group">
    {#each tabs as item (item.id)}
      <button
        class:active={tab === item.id}
        aria-pressed={tab === item.id}
        onclick={() => selectTab(item.id)}>{item.label}</button
      >
    {/each}
  </div>

  {#if tab === 'overview'}
    <div class="doctor-overview-grid">
      <section class="card doctor-profile-summary">
        <div class="profile-top">
          <Avatar name={patient.name} size={56} />
          <div>
            <span class="eyebrow">КАРТОЧКА ПАЦИЕНТА</span>
            <h2>{patient.name}</h2>
            <span class="status" class:observation={patient.status === 'observation'}
              >{patient.status === 'archived'
                ? 'В архиве'
                : patient.status === 'observation'
                  ? 'Наблюдение'
                  : 'Активен'}</span
            >
          </div>
        </div>
        <p class="profile-summary">
          {patient.medical_summary || 'Краткое описание пациента пока не добавлено.'}
        </p>
        <div class="profile-details">
          <span>Начало наблюдения<strong>{date(patient.observation_started)}</strong></span>
          <span>Аллергии<strong>{patient.allergies ?? 'Нет сведений'}</strong></span>
        </div>
        <div class="profile-actions">
          <button class="secondary" disabled={loading} onclick={onedit}
            >Редактировать карточку</button
          >
          {#if !patient.account_activated}
            <button class="secondary" disabled={loading} onclick={oninvite}
              >Подключить пациента</button
            >
          {/if}
        </div>
        {#if !patient.account_activated}
          <p class="muted small-text connection-state">
            Пациент ещё не принял приглашение. Передайте ему ссылку, чтобы открыть доступ к дневнику
            и истории.
          </p>
        {/if}
      </section>

      <section class="card patient-latest-summary" aria-labelledby="latest-summary-title">
        <div class="section-heading">
          <div>
            <h2 id="latest-summary-title">Последние показатели</h2>
            <p class="muted">По записям дневника пациента</p>
          </div>
          <HeartPulse size={20} />
        </div>
        <div class="patient-latest-grid">
          <div>
            <HeartPulse size={17} /><span>Настроение</span><strong
              >{patient.latest_mood === null ? 'Нет данных' : `${patient.latest_mood} / 10`}</strong
            >
          </div>
          <div>
            <Moon size={17} /><span>Сон</span><strong>{patient.latest_sleep === null ? 'Нет данных' : `${patient.latest_sleep} ч`}</strong>
          </div>
          <div>
            <CalendarDays size={17} /><span>Последняя запись</span><strong
              >{patient.last_entry_at ? date(patient.last_entry_at, true) : 'Пока нет'}</strong
            >
          </div>
        </div>
        <p class="muted small-text">Дневниковые самооценки не являются диагнозом.</p>
      </section>

      <ClinicalRecommendationsPanel {patient} doctor={true} {refreshKey} {onerror} />
      <AlertPanel {patient} doctor={true} activeOnly={true} {refreshKey} {onerror} />
    </div>
  {:else if tab === 'observation'}
    <div class="patient-tab-stack">
      <QuestionnairePanel {patient} doctor={true} {refreshKey} showHistory {onerror} />
      <section class="card dynamics-card">
        <div class="section-heading">
          <div>
            <h2>Динамика пациента</h2>
            <p class="muted">Дневниковые самооценки по отдельным шкалам</p>
          </div>
          <div class="period-selector" role="group" aria-label="Период графика">
            {#each [7, 30, 90] as value}
              <button
                class:active={period === value}
                aria-pressed={period === value}
                onclick={() => (period = value)}
              >
                {value === 90 ? '3 месяца' : `${value} дней`}
              </button>
            {/each}
          </div>
        </div>
        <div class="doctor-metric-grid">
          <MetricChart
            entries={periodEntries}
            metric="mood"
            label="Настроение"
            color="#219F90"
            unit="/ 10"
          />
          <MetricChart
            entries={periodEntries}
            metric="anxiety"
            label="Тревога"
            color="#8772BF"
            unit="/ 10"
          />
          <MetricChart
            entries={periodEntries}
            metric="sleep_hours"
            label="Продолжительность сна"
            color="#4D91BE"
            unit="ч"
            max={12}
          />
        </div>
        <p class="chart-footnote">
          Дневниковые самооценки не являются диагнозом или результатами клинической шкалы.
        </p>
      </section>
      <section class="card diary-history">
        <div class="section-heading">
          <div>
            <h2>Последние записи дневника</h2>
            <p class="muted">Настроение, тревога и продолжительность сна</p>
          </div>
          <ClipboardList size={20} />
        </div>
        {#each entries.slice(0, 10) as entry (entry.id)}
          <article class="diary-row">
            <div>
              <strong>{date(entry.measured_at, true)}</strong>
              <span class="diary-values"
                >Настроение {entry.mood}/10, тревога {entry.anxiety}/10, сон {entry.sleep_hours} ч</span
              >
            </div>
            {#if entry.comment}<p>{entry.comment}</p>{/if}
          </article>
        {:else}
          <p class="muted empty-state">Записей дневника пока нет.</p>
        {/each}
        {#if diaryCursor}
          <button class="secondary" disabled={diaryMoreBusy || loading} onclick={onloadDiary}>
            {diaryMoreBusy ? 'Загружаем записи' : 'Загрузить ещё записи'}
          </button>
        {/if}
      </section>
    </div>
  {:else if tab === 'diagnoses'}
    <div class="patient-tab-stack">
      <ClinicalPanel {patient} doctor={true} {refreshKey} {onerror} />
    </div>
  {:else if tab === 'treatment'}
    <div class="patient-tab-stack">
      <AppointmentsPanel doctor={true} {patient} {refreshKey} {onerror} />
      <TherapyPanel {patient} doctor={true} {refreshKey} {onerror} onshowdiagnoses={() => selectTab('diagnoses')} />
    </div>
  {:else}
    <div class="patient-tab-stack">
      <MedicalHistory history={patient.medical_history} />
      <AlertPanel {patient} doctor={true} {refreshKey} {onerror} />
      <section class="card patient-events-history">
        <div class="section-heading">
          <div>
            <h2>События и заметки</h2>
            <p class="muted">Внутренние записи видны только врачу</p>
          </div>
          <UserRound size={19} />
        </div>
        <div class="event-filter">
          <label for="doctor-event-filter">Тип записи</label>
          <select
            id="doctor-event-filter"
            bind:value={eventFilter}
            disabled={loading}
            onchange={() => void onretryEvents()}
          >
            <option value="all">Все события</option>
            {#each Object.entries(eventKinds) as [value, label]}
              <option {value}>{label}</option>
            {/each}
          </select>
        </div>
        {#if eventPageFailed}
          <button class="secondary" onclick={onretryEvents}>Повторить загрузку истории</button>
        {:else}
          <EventTimeline {events} doctor={true} />
          {#if eventMoreBusy}<p class="muted" role="status">Загружаем события</p>{/if}
          {#if eventCursor}
            <button class="secondary" disabled={eventMoreBusy || loading} onclick={onloadEvents}>
              Загрузить ещё события
            </button>
          {/if}
        {/if}
      </section>
      <ProfileHistory
        {changes}
        hasMore={changesCursor !== null}
        busy={changesMoreBusy || loading}
        onmore={onloadChanges}
      />
    </div>
  {/if}
</section>
