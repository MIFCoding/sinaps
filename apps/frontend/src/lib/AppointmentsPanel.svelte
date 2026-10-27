<script lang="ts">
  import { CalendarDays, LoaderCircle } from '@lucide/svelte';
  import { api, date, type Appointment, type Patient } from './api';

  let {
    doctor,
    patient,
    refreshKey,
    onerror,
  }: {
    doctor: boolean;
    patient: Patient | null;
    refreshKey: number;
    onerror: (cause: unknown) => void;
  } = $props();
  let items = $state<Appointment[]>([]);
  let startsAt = $state('');
  let endsAt = $state('');
  let note = $state('');
  let loading = $state(false);
  let saving = $state(false);
  let loadedKey = $state('');
  let editingId = $state<string | null>(null);
  let editStartsAt = $state('');
  let editEndsAt = $state('');

  const iso = (value: string) => {
    const parsed = new Date(value);
    if (!Number.isFinite(parsed.getTime())) throw new Error('Проверьте дату и время приёма');
    return parsed.toISOString();
  };
  const label = (item: Appointment) =>
    `С ${date(item.starts_at, true)} до ${new Intl.DateTimeFormat('ru-RU', {
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(item.ends_at))}`;

  async function load() {
    if (loading) return;
    loading = true;
    try {
      const page = await api('list_appointments');
      items = page.items;
      loadedKey = `${patient?.id ?? 'account'}:${refreshKey}`;
    } catch (cause) {
      onerror(cause);
    } finally {
      loading = false;
    }
  }
  async function createSlot() {
    if (saving || !startsAt || !endsAt) return;
    saving = true;
    try {
      await api('create_appointment', {
        body: {
          client_id: crypto.randomUUID(),
          patient_id: null,
          starts_at: iso(startsAt),
          ends_at: iso(endsAt),
          note,
        },
      });
      startsAt = '';
      endsAt = '';
      note = '';
      await load();
    } catch (cause) {
      onerror(cause);
    } finally {
      saving = false;
    }
  }
  async function book(item: Appointment) {
    if (saving) return;
    saving = true;
    try {
      await api('book_appointment', {
        params: { id: item.id },
        body: { client_id: crypto.randomUUID() },
      });
      await load();
    } catch (cause) {
      onerror(cause);
    } finally {
      saving = false;
    }
  }
  async function cancel(item: Appointment) {
    if (saving) return;
    saving = true;
    try {
      await api('cancel_appointment', {
        params: { id: item.id },
        body: { expected_version: item.version },
      });
      await load();
    } catch (cause) {
      onerror(cause);
    } finally {
      saving = false;
    }
  }
  function startReschedule(item: Appointment) {
    editingId = item.id;
    editStartsAt = new Date(item.starts_at).toISOString().slice(0, 16);
    editEndsAt = new Date(item.ends_at).toISOString().slice(0, 16);
  }
  async function reschedule(item: Appointment) {
    if (saving || !editStartsAt || !editEndsAt) return;
    saving = true;
    try {
      await api('reschedule_appointment', {
        params: { id: item.id },
        body: {
          expected_version: item.version,
          starts_at: iso(editStartsAt),
          ends_at: iso(editEndsAt),
        },
      });
      editingId = null;
      await load();
    } catch (cause) {
      onerror(cause);
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    const key = `${patient?.id ?? 'account'}:${refreshKey}`;
    if (key !== loadedKey) void load();
  });
</script>

<section class="card appointments-panel" aria-labelledby="appointments-title">
  <div class="section-heading">
    <div>
      <h2 id="appointments-title">Приёмы</h2>
      <p class="muted">
        {doctor ? 'Свободные интервалы и назначенные встречи' : 'Запись к ведущему врачу'}
      </p>
    </div>
    <CalendarDays size={20} />
  </div>
  {#if doctor}
    <form
      class="appointment-form"
      onsubmit={(event) => {
        event.preventDefault();
        void createSlot();
      }}
    >
      <label>Начало<input type="datetime-local" bind:value={startsAt} required /></label>
      <label>Окончание<input type="datetime-local" bind:value={endsAt} required /></label>
      <label class="appointment-note"
        >Комментарий<input bind:value={note} maxlength="1000" placeholder="Необязательно" /></label
      >
      <div class="appointment-form-actions">
        <button class="primary" type="submit" disabled={saving}
          ><CalendarDays size={17} />Создать свободный интервал</button
        >
      </div>
    </form>
  {/if}
  {#if loading}
    <div class="empty-state" role="status">
      <LoaderCircle class="spin" size={22} />Загружаем расписание
    </div>
  {:else if items.length === 0}
    <p class="muted empty-state">Свободных или назначенных приёмов пока нет.</p>
  {:else}
    <div class="appointments-list">
      {#each items as item (item.id)}
        <article class="appointment-row">
          <div>
            <strong>{label(item)}</strong>
            <small>Пациент: {item.patient_name ?? 'Свободный интервал'}. Врач: {item.doctor_name}</small>
            {#if item.note}<p class="muted">{item.note}</p>{/if}
          </div>
          {#if !doctor && item.status === 'available'}
            <button class="secondary" disabled={saving} onclick={() => book(item)}
              >Записаться</button
            >
          {:else}
            <div class="appointment-actions">
              {#if item.status === 'scheduled'}
                <span class="status">Запланирован</span>
              {:else if item.status === 'cancelled'}
                <span class="status">Отменён</span>
              {/if}
              {#if item.status !== 'cancelled' && (doctor || item.patient_id === patient?.id)}
                <button class="secondary" disabled={saving} onclick={() => cancel(item)}
                  >Отменить</button
                >
              {/if}
              {#if doctor && item.status !== 'cancelled'}
                <button class="secondary" disabled={saving} onclick={() => startReschedule(item)}
                  >Перенести</button
                >
              {/if}
            </div>
          {/if}
        </article>
        {#if doctor && editingId === item.id}
          <form
            class="appointment-form appointment-reschedule"
            onsubmit={(event) => {
              event.preventDefault();
              void reschedule(item);
            }}
          >
            <label
              >Новое начало<input type="datetime-local" bind:value={editStartsAt} required /></label
            >
            <label
              >Новое окончание<input
                type="datetime-local"
                bind:value={editEndsAt}
                required
              /></label
            >
            <button class="primary" type="submit" disabled={saving}>Сохранить перенос</button>
            <button
              class="secondary"
              type="button"
              disabled={saving}
              onclick={() => {
                editingId = null;
              }}>Отмена</button
            >
          </form>
        {/if}
      {/each}
    </div>
  {/if}
</section>
