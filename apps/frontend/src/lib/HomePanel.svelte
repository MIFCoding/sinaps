<script lang="ts">
  import { Plus, Users } from '@lucide/svelte';
  import Greeting from './Greeting.svelte';
  import MetricChart from './MetricChart.svelte';
  import AlertPanel from './AlertPanel.svelte';
  import UpcomingAppointment from './UpcomingAppointment.svelte';
  import { date, type User, type Patient, type DiaryEntry } from './api';
  let {
    user,
    patient = null,
    entries,
    onview,
    ondiary,
    oncreate,
    onpatient,
    refreshKey,
    onerror,
  }: {
    user: User;
    patient: Patient | null;
    entries: DiaryEntry[];
    onview: (view: string) => void;
    ondiary: () => void;
    oncreate: () => void;
    onpatient: (id: string) => Promise<void>;
    refreshKey: number;
    onerror: (cause: unknown) => void;
  } = $props();
  let doctor = $derived(user.role === 'doctor');
  let days = $derived(
    patient
      ? Math.max(
          1,
          Math.floor(
            (Date.now() - Date.parse(`${patient.observation_started}T00:00:00`)) / 86400000,
          ) + 1,
        )
      : 0,
  );
  const number = (value: number | null) =>
    value === null ? 'Нет данных' : new Intl.NumberFormat('ru-RU').format(value);
</script>

<div class="home-panel" class:doctor-home={doctor}>
  <Greeting {user} />
  {#if doctor}
    <UpcomingAppointment {doctor} {patient} {refreshKey} {onview} {onerror} />
    <section class="card quick-actions">
      <h2>Быстрые действия</h2>
      <div class="action-columns doctor-actions">
        <button class="primary" onclick={oncreate}><Plus size={18} />Новый пациент</button>
        <button class="secondary" onclick={() => onview('patients')}
          ><Users size={18} />Найти пациента</button
        >
      </div>
    </section>
    <AlertPanel {doctor} patient={null} {refreshKey} {onpatient} {onerror} />
  {:else}
    <section class="card home-metrics">
      <div>
        <span>Самочувствие</span><strong>{patient?.latest_mood === null || !patient ? 'Нет данных' : `${number(patient.latest_mood)} / 10`}</strong><small
          >Последняя запись</small
        >
      </div>
      <div>
        <span>Сон</span><strong>{patient?.latest_sleep === null || !patient ? 'Нет данных' : `${number(patient.latest_sleep)} ч`}</strong><small
          >За прошедшую ночь</small
        >
      </div>
      <div>
        <span>Дней наблюдения</span><strong>{days}</strong><small
          >{patient
            ? `С ${date(patient.observation_started)}`
            : 'Данные появятся после связи'}</small
        >
      </div>
    </section>
    <div class="patient-home-columns">
      <div class="home-stack">
        <UpcomingAppointment {doctor} {patient} {refreshKey} {onview} {onerror} />
        <section class="card daily-entry">
          <h2>Ежедневная запись</h2>
          <p class="muted">
            Заполните сон, настроение и тревогу. Каждый показатель появится в своём графике.
          </p>
          <button class="primary full-width" onclick={ondiary}>Заполнить дневник</button>
        </section>
      </div>
      <div class="home-stack">
        <section class="card home-dynamics">
          <h2>Моя динамика</h2>
          <MetricChart
            entries={entries.slice(-12)}
            metric="mood"
            label="Настроение"
            color="var(--teal)"
            unit="из 10"
          />
          <button class="secondary full-width" onclick={() => onview('dynamics')}
            >Все показатели и рубежи</button
          >
        </section>
      </div>
    </div>
  {/if}
</div>
