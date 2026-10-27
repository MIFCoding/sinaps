<script lang="ts">
  import { CalendarDays, ChevronRight } from '@lucide/svelte';
  import { api, type Appointment, type Patient } from './api';

  let {
    doctor,
    patient = null,
    refreshKey,
    onview,
    onerror,
  }: {
    doctor: boolean;
    patient?: Patient | null;
    refreshKey: number;
    onview: (view: string) => void;
    onerror: (cause: unknown) => void;
  } = $props();
  let items = $state<Appointment[]>([]);
  let loadedKey = $state('');
  let next = $derived(
    items
      .filter((item) => item.status === 'scheduled' && Date.parse(item.starts_at) > Date.now())
      .sort((left, right) => Date.parse(left.starts_at) - Date.parse(right.starts_at))[0] ?? null,
  );
  const formatDate = (item: Appointment) =>
    capitalize(
      new Intl.DateTimeFormat('ru-RU', {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
        hour: '2-digit',
        minute: '2-digit',
      }).format(new Date(item.starts_at)),
    );
  const capitalize = (value: string) =>
    value ? value.charAt(0).toLocaleUpperCase('ru-RU') + value.slice(1) : value;
  async function load() {
    const key = `${patient?.id ?? 'doctor'}:${refreshKey}`;
    if (loadedKey === key) return;
    try {
      const page = await api('list_appointments');
      items = page.items;
      loadedKey = key;
    } catch (cause) {
      onerror(cause);
    }
  }
  $effect(() => {
    patient?.id;
    refreshKey;
    void load();
  });
</script>

{#if next}
  <section class="upcoming-appointment" aria-label="Ближайший приём">
    <span class="upcoming-icon"><CalendarDays size={18} /></span>
    <span class="upcoming-copy">
      <span class="eyebrow">БЛИЖАЙШИЙ ПРИЁМ</span>
      <strong>{formatDate(next)}</strong>
      <small>{doctor ? (next.patient_name ?? 'Пациент') : next.doctor_name}</small>
    </span>
    <button class="text-link" onclick={() => onview('appointments')}>
      Расписание<ChevronRight size={15} />
    </button>
  </section>
{/if}
