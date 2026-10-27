<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Plus, Search, LoaderCircle } from '@lucide/svelte';
  import Avatar from './Avatar.svelte';
  import {
    api,
    date,
    statusLabel,
    type Patient,
    type PatientPage,
    type DirectoryQuery,
  } from './api';
  let {
    refreshKey = 0,
    selected,
    onselect,
    oncreate,
  }: {
    refreshKey?: number;
    selected: Patient | null;
    onselect: (id: string) => Promise<boolean>;
    oncreate: () => void;
  } = $props();
  let search = $state(''),
    filter = $state('all');
  let items = $state<Patient[]>([]),
    matchingCount = $state(0);
  let cursor = $state<PatientPage['next_cursor']>(null),
    loading = $state(false),
    error = $state('');
  let generation = 0,
    alive = true;
  let activeQuery = { search: '', status: 'all' };
  async function selectPatient(id: string) {
    if (alive) await onselect(id);
  }
  async function load(query: typeof activeQuery, token: number, more = false) {
    if (more && (loading || !cursor)) return;
    loading = true;
    error = '';
    const parameters: NonNullable<DirectoryQuery> = {
      limit: 20,
      search: query.search,
      status: query.status,
    };
    if (more && cursor) {
      parameters.before_name = cursor.name;
      parameters.before_id = cursor.id;
    }
    try {
      const page = await api('list_patients_page', {
        query: parameters,
      });
      if (!alive || token !== generation) return;
      const known = new Set(items.map((patient) => patient.id));
      items = more
        ? [...items, ...page.items.filter((patient) => !known.has(patient.id))]
        : page.items;
      matchingCount = page.matching_count;
      cursor = page.next_cursor;
    } catch (cause) {
      if (alive && token === generation)
        error = cause instanceof Error ? cause.message : 'Не удалось загрузить пациентов';
    } finally {
      if (alive && token === generation) loading = false;
    }
  }
  function schedule(term: string, status: string, _refreshKey: number) {
    const token = ++generation;
    activeQuery = { search: term, status };
    items = [];
    cursor = null;
    matchingCount = 0;
    loading = true;
    error = '';
    // Новый поиск не принимает запоздавшие ответы предыдущего запроса.
    const timer = window.setTimeout(() => void load({ search: term, status }, token), 200);
    return () => clearTimeout(timer);
  }
  $effect(() => schedule(search.trim(), filter, refreshKey));
  onDestroy(() => {
    alive = false;
    generation++;
  });
</script>

<div class="patient-directory">
  <label class="search-field"
    ><Search size={20} /><input
      aria-label="Поиск пациента"
      placeholder="Поиск по ФИО"
      maxlength="120"
      bind:value={search}
    /></label
  >
  <button class="primary create-patient" onclick={oncreate}><Plus size={18} />Новый пациент</button>
  <div class="filters">
    {#each [['all', 'Все'], ['active', 'Активные'], ['observation', 'На наблюдении'], ['archived', 'Архив']] as [value, label]}
      <button class:chosen={filter === value} onclick={() => (filter = value)}>{label}</button>
    {/each}
  </div>
  <div class="directory-columns">
    <section class="card patient-list-card">
      <div class="section-heading">
        <p class="directory-result-count" role="status">Найдено: {matchingCount}</p>
        {#if loading}<LoaderCircle class="spin" size={18} aria-label="Загрузка пациентов" />{/if}
      </div>
      {#if error}<div class="feedback error" role="alert">
          {error}<button class="text-button" onclick={() => load(activeQuery, generation)}
            >Повторить</button
          >
        </div>{/if}
      {#if items.length}
        <div class="patient-table-scroll">
          <table class="patient-table">
            <thead>
              <tr
                ><th scope="col">Пациент</th><th scope="col">Статус</th><th scope="col"
                  >Начало наблюдения</th
                ></tr
              >
            </thead>
            <tbody>
              {#each items as patient (patient.id)}
                <tr class:selected={selected?.id === patient.id}>
                  <td data-label="Пациент">
                    <button
                      class="patient-table-select"
                      aria-pressed={selected?.id === patient.id}
                      onclick={() => selectPatient(patient.id)}
                    >
                      <Avatar name={patient.name} size={40} />
                      <span><strong>{patient.name}</strong><small>Открыть наблюдение</small></span>
                    </button>
                  </td>
                  <td data-label="Статус">
                    <span class="patient-table-status"
                      >{patient.account_activated
                        ? statusLabel(patient.status)
                        : 'Не подключён'}</span
                    >
                  </td>
                  <td data-label="Начало наблюдения">
                    <time datetime={patient.observation_started}
                      >{date(patient.observation_started)}</time
                    >
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else if !loading && !error}<div class="empty-state">
          По этому запросу пациентов нет
        </div>{/if}
      {#if loading && !items.length}<p class="muted" role="status">Загружаем пациентов</p>{/if}
      {#if items.length}<p class="muted directory-count">
          Показано {items.length} из {matchingCount}
        </p>{/if}
      {#if cursor}<button
          class="secondary full-width"
          disabled={loading}
          onclick={() => load(activeQuery, generation, true)}>Показать ещё</button
        >{/if}
    </section>
  </div>
</div>
