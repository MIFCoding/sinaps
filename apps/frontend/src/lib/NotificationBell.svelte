<script lang="ts">
  import { onMount } from 'svelte';
  import { Bell, CheckCheck, LoaderCircle, X } from '@lucide/svelte';
  import { api, eventDateLabel, type Notification, type NotificationPage } from './api';
  let {
    doctor,
    onopen,
    onmessageschange,
  }: {
    doctor: boolean;
    onopen: (notification: Notification) => Promise<void>;
    onmessageschange?: (count: number) => void;
  } = $props();
  let opened = $state(false),
    loading = $state(false),
    busy = $state(false);
  let items = $state<Notification[]>([]),
    unread = $state(0),
    cursor = $state<NotificationPage['next_cursor']>(null);
  let error = $state('');
  let alive = true;
  async function load(more = false) {
    if (loading || (more && !cursor)) return;
    loading = true;
    error = '';
    try {
      const query =
        more && cursor ? { before: cursor.before, before_id: cursor.before_id } : undefined;
      const page = await api('list_notifications', {
        query: query,
      });
      if (!alive) return;
      const known = new Set(items.map((item) => item.id));
      items = more ? [...items, ...page.items.filter((item) => !known.has(item.id))] : page.items;
      unread = page.unread_count;
      onmessageschange?.(page.unread_messages_count);
      cursor = page.next_cursor;
    } catch (cause) {
      if (alive)
        error = cause instanceof Error ? cause.message : 'Не удалось загрузить уведомления';
    } finally {
      if (alive) loading = false;
    }
  }
  export async function refreshNotifications() {
    await load();
  }
  onMount(() => {
    void load();
    const poll = () => {
      if (document.visibilityState === 'visible' && !opened && !busy) void load();
    };
    const timer = window.setInterval(poll, 30000);
    const messagesRead = () => void load();
    document.addEventListener('visibilitychange', poll);
    window.addEventListener('synapse:messages-read', messagesRead);
    return () => {
      alive = false;
      clearInterval(timer);
      document.removeEventListener('visibilitychange', poll);
      window.removeEventListener('synapse:messages-read', messagesRead);
    };
  });
  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    return {
      destroy() {
        node.close();
        previous?.focus();
      },
    };
  }
  async function select(item: Notification) {
    busy = true;
    error = '';
    try {
      const result = await api('read_notification', {
        params: { id: item.id },
      });
      if (!item.read_at) unread = Math.max(0, unread - 1);
      items = items.map((current) =>
        current.id === item.id ? { ...current, read_at: result.read_at } : current,
      );
      await onopen(item);
      await load();
      opened = false;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Не удалось открыть уведомление';
    } finally {
      busy = false;
    }
  }
  async function readAll() {
    busy = true;
    error = '';
    try {
      await api('read_all_notifications');
      await load();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Не удалось отметить уведомления';
    } finally {
      busy = false;
    }
  }
</script>

<button
  class="icon-button notification-bell"
  aria-label={unread ? `Уведомления, непрочитанных: ${unread}` : 'Уведомления'}
  title="Уведомления"
  onclick={() => {
    opened = true;
    void load();
  }}
>
  <img src="/design/bell.svg" alt="" />
  {#if unread}<span class="notification-count">{unread > 99 ? '99+' : unread}</span>{/if}
</button>

{#if opened}
  <dialog
    class="modal card notifications-dialog"
    aria-labelledby="notifications-title"
    use:dialog
    oncancel={(event) => {
      event.preventDefault();
      if (!busy) opened = false;
    }}
  >
    <div class="section-heading">
      <h2 id="notifications-title">Уведомления</h2>
      <button
        class="icon-button"
        aria-label="Закрыть уведомления"
        disabled={busy}
        onclick={() => (opened = false)}><X size={21} /></button
      >
    </div>
    {#if unread}<button
        class="text-link read-notifications"
        disabled={busy || loading}
        onclick={readAll}><CheckCheck size={18} />Отметить все прочитанными</button
      >{/if}
    {#if error}<div class="feedback error" role="alert">
        {error}<button class="text-button" disabled={loading || busy} onclick={() => load()}
          >Повторить</button
        >
      </div>{/if}
    <div class="notifications-list">
      {#each items as item (item.id)}
        <button
          class="notification-item"
          class:unread={!item.read_at}
          disabled={busy}
          onclick={() => select(item)}
        >
          <span class="notification-symbol"><Bell size={18} /></span>
          <span
            ><strong
              >{item.kind === 'diary_entry'
                ? 'Новая запись дневника'
                : item.kind === 'published_event'
                  ? 'Новая запись врача'
                  : item.kind === 'prescription_published'
                    ? 'Новое назначение врача'
                    : item.kind === 'critical_alert'
                      ? 'Критический алерт'
                        : item.kind === 'chat_message'
                          ? 'Новое сообщение'
                          : item.kind === 'appointment' ||
                              item.kind.startsWith('appointment_')
                            ? item.kind === 'appointment_cancelled'
                              ? 'Приём отменён'
                              : item.kind === 'appointment_rescheduled'
                                ? 'Приём перенесён'
                                : item.kind === 'appointment_booked'
                                  ? 'Пациент записался на приём'
                                  : 'Назначен приём'
                        : 'Нежелательное явление'}</strong
            >{#if doctor}<span>{item.patient_name}</span>{/if}<small
              >{eventDateLabel(item.created_at)}</small
            >{#if !item.read_at}<small class="unread-label">Не прочитано</small>{/if}</span
          >
        </button>
      {:else}{#if !loading && !error}<p class="empty-state">Уведомлений пока нет</p>{/if}{/each}
    </div>
    {#if loading}<div class="notification-loading" role="status">
        <LoaderCircle class="spin" size={20} />Загружаем уведомления
      </div>{/if}
    {#if cursor}<button
        class="secondary full-width"
        disabled={loading || busy}
        onclick={() => load(true)}>Показать ещё</button
      >{/if}
  </dialog>
{/if}
