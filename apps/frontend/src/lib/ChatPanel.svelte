<script lang="ts">
  import { tick } from 'svelte';
  import { ArrowDown, LoaderCircle, Send } from '@lucide/svelte';
  import { api, date, type ChatMessage, type ChatPage, type Patient } from './api';
  import Avatar from './Avatar.svelte';

  let {
    patient,
    doctor,
    userId,
    refreshKey,
    onerror,
  }: {
    patient: Patient;
    doctor: boolean;
    userId: string;
    refreshKey: number;
    onerror: (cause: unknown) => void;
  } = $props();

  let messages = $state<ChatMessage[]>([]);
  let nextCursor = $state<ChatPage['next_cursor']>(null);
  let unreadCount = $state(0);
  let body = $state('');
  let loading = $state(false);
  let sending = $state(false);
  let loadingMore = $state(false);
  let polling = $state(false);
  let loadedKey = $state('');
  let chatList = $state<HTMLDivElement | undefined>(undefined);

  async function scrollToLatest() {
    await tick();
    if (chatList) chatList.scrollTop = chatList.scrollHeight;
  }

  async function load() {
    if (loading) return;
    loading = true;
    try {
      const page = await api('list_messages', { params: { id: patient.id } });
      messages = page.items.reverse();
      nextCursor = page.next_cursor;
      unreadCount = page.unread_count;
      if (unreadCount > 0) {
        await api('read_messages', { params: { id: patient.id } });
        unreadCount = 0;
        window.dispatchEvent(new CustomEvent('synapse:messages-read'));
      }
      loadedKey = `${patient.id}:${refreshKey}`;
      await scrollToLatest();
    } catch (cause) {
      onerror(cause);
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (!nextCursor || loadingMore) return;
    loadingMore = true;
    const previousHeight = chatList?.scrollHeight ?? 0;
    const previousTop = chatList?.scrollTop ?? 0;
    try {
      const page = await api('list_messages', {
        params: { id: patient.id },
        query: {
          before: nextCursor.before ?? undefined,
          before_id: nextCursor.before_id ?? undefined,
        },
      });
      messages = [...page.items.reverse(), ...messages];
      nextCursor = page.next_cursor;
      await tick();
      if (chatList) chatList.scrollTop = previousTop + chatList.scrollHeight - previousHeight;
    } catch (cause) {
      onerror(cause);
    } finally {
      loadingMore = false;
    }
  }

  async function poll() {
    if (polling || loading || document.visibilityState !== 'visible') return;
    polling = true;
    const wasAtLatest = chatList
      ? chatList.scrollHeight - chatList.scrollTop - chatList.clientHeight < 80
      : true;
    try {
      const page = await api('list_messages', { params: { id: patient.id } });
      const previousCount = messages.length;
      const known = new Map(messages.map((message) => [message.id, message]));
      for (const message of page.items) known.set(message.id, message);
      messages = [...known.values()].sort(
        (a, b) => Date.parse(a.created_at) - Date.parse(b.created_at) || a.id.localeCompare(b.id),
      );
      if (wasAtLatest && messages.length > previousCount) await scrollToLatest();
      unreadCount = page.unread_count;
      if (unreadCount > 0) {
        await api('read_messages', { params: { id: patient.id } });
        unreadCount = 0;
      }
    } catch (cause) {
      onerror(cause);
    } finally {
      polling = false;
    }
  }

  async function send() {
    const text = body.trim();
    if (!text || sending) return;
    sending = true;
    try {
      const message = await api('send_message', {
        params: { id: patient.id },
        body: { client_id: crypto.randomUUID(), body: text },
      });
      messages = [...messages, message];
      body = '';
      await scrollToLatest();
    } catch (cause) {
      onerror(cause);
    } finally {
      sending = false;
    }
  }

  $effect(() => {
    const key = `${patient.id}:${refreshKey}`;
    if (key !== loadedKey) void load();
  });

  $effect(() => {
    patient.id;
    const timer = window.setInterval(() => void poll(), 30000);
    const resume = () => {
      if (document.visibilityState === 'visible') void poll();
    };
    document.addEventListener('visibilitychange', resume);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', resume);
    };
  });
</script>

<section class="card chat-panel" aria-labelledby="chat-title">
  <div class="chat-header">
    <Avatar name={doctor ? patient.name : patient.doctor_name} size={44} />
    <div>
      <h2 id="chat-title">{doctor ? patient.name : patient.doctor_name}</h2>
      <p class="muted">{doctor ? 'Пациент' : 'Лечащий врач'}</p>
    </div>
    {#if unreadCount}<span class="count">{unreadCount}</span>{/if}
  </div>
  {#if loading}
    <div class="empty-state" role="status">
      <LoaderCircle class="spin" size={24} />Загружаем диалог
    </div>
  {:else}
    <div class="chat-list" bind:this={chatList} aria-live="polite">
      {#if nextCursor}<button class="text-link chat-more" onclick={loadMore} disabled={loadingMore}>
          <ArrowDown size={16} />{loadingMore ? 'Загружаем сообщения' : 'Более ранние сообщения'}
        </button>{/if}
      {#if messages.length === 0}<p class="muted chat-empty">
          Сообщений пока нет. Напишите первым.
        </p>{/if}
      {#each messages as message}
        <article class:own={message.sender_id === userId} class="chat-message">
          {#if message.sender_id !== userId}<Avatar name={message.sender_name} size={28} />{/if}
          <div class="chat-bubble">
            <div class="chat-message-meta">{message.sender_name}</div>
            <p>{message.body}</p>
            <time datetime={message.created_at}>{date(message.created_at, true)}</time>
          </div>
        </article>
      {/each}
    </div>
    <form
      class="chat-compose"
      onsubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <textarea
        bind:value={body}
        rows="1"
        maxlength="4000"
        placeholder="Напишите сообщение"
        aria-label="Сообщение"></textarea>
      <button
        class="primary chat-send"
        type="submit"
        aria-label="Отправить сообщение"
        disabled={!body.trim() || sending}
      >
        {#if sending}<LoaderCircle class="spin" size={18} />{:else}<Send size={18} />{/if}
      </button>
    </form>
  {/if}
</section>
