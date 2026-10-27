<script lang="ts">
  import { onMount } from 'svelte';
  import { LoaderCircle, MessageCircle, Search } from '@lucide/svelte';
  import Avatar from './Avatar.svelte';
  import { api } from './api';

  let {
    onselect,
    onerror,
  }: {
    onselect: (id: string) => Promise<boolean>;
    onerror: (cause: unknown) => void;
  } = $props();

  type Thread = { patient_id: string; patient_name: string; unread_count: number };
  let threads = $state<Thread[]>([]);
  let loading = $state(true);
  let searchQuery = $state('');
  let filteredThreads = $derived.by(() => {
    const terms = searchQuery.trim().toLocaleLowerCase('ru-RU').split(/\s+/).filter(Boolean);
    return threads.filter((thread) => {
      const name = thread.patient_name.toLocaleLowerCase('ru-RU');
      return terms.every((term) => name.includes(term));
    });
  });

  async function load() {
    loading = true;
    try {
      threads = await api('list_chat_threads');
    } catch (cause) {
      onerror(cause);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    const timer = window.setInterval(() => void load(), 30000);
    return () => window.clearInterval(timer);
  });
</script>

<section class="card chat-threads" aria-labelledby="chat-threads-title">
  <div class="section-heading">
    <div>
      <h2 id="chat-threads-title">Диалоги</h2>
      <p class="muted">Все пациенты, с которыми можно переписываться</p>
    </div>
    <MessageCircle size={20} />
  </div>
  <label class="chat-thread-search">
    <Search size={18} aria-hidden="true" />
    <input
      bind:value={searchQuery}
      type="search"
      placeholder="Поиск пациента по ФИО"
      aria-label="Поиск пациента по ФИО"
    />
  </label>
  {#if loading}<p class="muted" role="status">
      <LoaderCircle class="spin" size={18} />Загружаем диалоги
    </p>
  {:else if !threads.length}<p class="empty-state">Пациентов для переписки пока нет.</p>
  {:else if !filteredThreads.length}<p class="empty-state">По этому ФИО диалогов не найдено.</p>
  {:else}
    {#each filteredThreads as thread (thread.patient_id)}
      <button
        class="person-row chat-thread"
        onclick={() => onselect(thread.patient_id)}
        aria-label={`Открыть диалог с ${thread.patient_name}`}
      >
        <Avatar name={thread.patient_name} size={48} />
        <span><strong>{thread.patient_name}</strong><small>Личный диалог</small></span>
        {#if thread.unread_count}<span class="thread-unread"
            >{thread.unread_count > 99 ? '99+' : thread.unread_count}</span
          >{/if}
      </button>
    {/each}
  {/if}
</section>
