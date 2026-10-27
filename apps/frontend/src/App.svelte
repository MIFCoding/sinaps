<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { exit, onBackButtonPress } from '@tauri-apps/api/app';
  import { isTauri } from '@tauri-apps/api/core';
  import {
    Activity,
    ArrowRight,
    Check,
    ChevronRight,
    ClipboardList,
    Home,
    LogOut,
    Plus,
    RefreshCw,
    Users,
    X,
    CalendarDays,
    CalendarClock,
    LoaderCircle,
    MessageCircle,
    Pill,
  } from '@lucide/svelte';
  import MetricChart from './lib/MetricChart.svelte';
  import PatientManager from './lib/PatientManager.svelte';
  import ProfileHistory from './lib/ProfileHistory.svelte';
  import MedicalHistory from './lib/MedicalHistory.svelte';
  import BasicInformation from './lib/BasicInformation.svelte';
  import ClinicalPanel from './lib/ClinicalPanel.svelte';
  import ClinicalRecommendationsPanel from './lib/ClinicalRecommendationsPanel.svelte';
  import AuthPanel from './lib/AuthPanel.svelte';
  import AdminPanel from './lib/AdminPanel.svelte';
  import Brand from './lib/Brand.svelte';
  import Avatar from './lib/Avatar.svelte';
  import HomePanel from './lib/HomePanel.svelte';
  import Greeting from './lib/Greeting.svelte';
  import QuestionnairePanel from './lib/QuestionnairePanel.svelte';
  import TherapyPanel from './lib/TherapyPanel.svelte';
  import AlertPanel from './lib/AlertPanel.svelte';
  import PatientDirectory from './lib/PatientDirectory.svelte';
  import EventTimeline from './lib/EventTimeline.svelte';
  import NotificationBell from './lib/NotificationBell.svelte';
  import ChatPanel from './lib/ChatPanel.svelte';
  import ChatThreads from './lib/ChatThreads.svelte';
  import AppointmentsPanel from './lib/AppointmentsPanel.svelte';
  import DoctorPatientWorkspace from './lib/DoctorPatientWorkspace.svelte';
  import {
    pullToRefresh,
    PULL_INDICATOR_THRESHOLD,
    PULL_REFRESH_THRESHOLD,
    type PullPhase,
  } from './lib/pullToRefresh';
  import {
    api,
    ApiError,
    date,
    localDateTime,
    eventDateTime,
    eventKinds,
    statusLabel,
    type User,
    type Patient,
    type DiaryEntry,
    type HistoryCursor,
    type ObservationEvent,
    type ProfileChange,
    type ChangesPage,
    type EventKind,
    type Notification,
  } from './lib/api';
  let user = $state<User | null>(null),
    booting = $state(true),
    loading = $state(false),
    refreshing = $state(false);
  let authInvite = $state(location.hash);
  let error = $state(''),
    notice = $state('');
  let manager = $state<'create' | 'edit' | 'invite' | null>(null);
  let managerTarget = $state<Patient | null>(null);
  let changes = $state<ProfileChange[]>([]);
  let changesCursor = $state<ChangesPage['next_cursor']>(null),
    changesMoreBusy = $state(false);
  let patients = $state<Patient[]>([]),
    selected = $state<Patient | null>(null);
  let patientTotal = $state(0),
    directoryRefreshKey = $state(0);
  let entries = $state<DiaryEntry[]>([]),
    events = $state<ObservationEvent[]>([]);
  let recentEvents = $state<ObservationEvent[]>([]);
  let diaryCursor = $state<HistoryCursor | null>(null),
    eventCursor = $state<HistoryCursor | null>(null);
  let diaryMoreBusy = $state(false),
    eventMoreBusy = $state(false);
  let eventPageFailed = $state(false),
    eventRetryReset = false;
  let eventRequest = 0,
    diaryRequest = 0;
  let view = $state('home'),
    period = $state(30);
  let patientTab = $state<'overview' | 'observation' | 'diagnoses' | 'treatment' | 'history'>('overview');
  let modal = $state<'diary' | 'note' | null>(null),
    saving = $state(false);
  let mood = $state(5),
    anxiety = $state(5),
    sleep = $state(7),
    comment = $state(''),
    measuredAt = $state(localDateTime());
  let noteTitle = $state(''),
    noteBody = $state(''),
    patientVisible = $state(false);
  let noteKind = $state<EventKind>('note'),
    noteSource = $state(''),
    noteOccurredAt = $state(localDateTime());
  let eventFilter = $state<EventKind | 'all'>('all');
  let filteredEvents = $derived(
    events.filter((event) => eventFilter === 'all' || event.kind === eventFilter),
  );
  let submissionId = crypto.randomUUID();
  let pullDistance = $state(0),
    gestureBusy = $state(false);
  let pullPhase = $state<PullPhase>('idle');
  let notificationBell = $state<{
    refreshNotifications: () => Promise<void>;
  } | null>(null);
  let unreadMessages = $state(0);
  let generation = 0;
  let periodEntries = $derived(
    entries.filter((e) => Date.parse(e.measured_at) >= Date.now() - period * 86400000),
  );
  let doctor = $derived(user?.role === 'doctor');
  let pageTitle = $derived(
    view === 'home'
      ? 'Главная'
      : view === 'patients'
        ? 'Пациенты'
        : view === 'doctors'
          ? 'Врачи'
          : view === 'dynamics'
            ? 'Динамика'
            : view === 'profile'
              ? doctor
                ? 'Карточка пациента'
                : 'Медкарта'
              : view === 'messages'
                ? 'Сообщения'
                : view === 'appointments'
                  ? 'Приёмы'
                  : view === 'observation'
                    ? 'Наблюдение пациента'
                    : 'История наблюдения',
  );
  let patientNavigation = $derived(
    doctor
      ? [
          { id: 'home', label: 'Главная', icon: Home, asset: 'home' },
          { id: 'patients', label: 'Пациенты', icon: Users, asset: 'users' },
          { id: 'messages', label: 'Сообщения', icon: MessageCircle, asset: 'chat' },
        ]
      : [
          { id: 'home', label: 'Главная', icon: Home, asset: 'home' },
          { id: 'dynamics', label: 'Динамика', icon: Activity },
          { id: 'messages', label: 'Сообщения', icon: MessageCircle, asset: 'chat' },
          { id: 'appointments', label: 'Приёмы', icon: CalendarClock },
          { id: 'medications', label: 'Лекарства', icon: Pill, asset: 'pill' },
        ],
  );
  let activeNavigation = $derived(
    doctor && ['dynamics', 'appointments', 'history', 'observation', 'profile'].includes(view)
      ? 'patients'
      : !doctor && view === 'history'
        ? 'dynamics'
        : view,
  );
  type NavigationSnapshot = {
    view: string;
    patientId: string | null;
    patientTab: 'overview' | 'observation' | 'diagnoses' | 'treatment' | 'history';
  };
  let navigationHistory: NavigationSnapshot[] = [];
  let currentScreen: NavigationSnapshot | null = null;
  let restoringNavigation = false;
  let lastScreenKey = '';
  let backButtonListener: { unregister: () => Promise<void> } | null = null;
  function getScreenSnapshot(): NavigationSnapshot {
    const patientContextVisible = view !== 'home' && view !== 'patients';
    const patientId = patientContextVisible ? (selected?.id ?? null) : null;
    const tab = doctor && patientId && view !== 'messages' ? patientTab : 'overview';
    return { view, patientId, patientTab: tab };
  }
  function screenKey(screen: NavigationSnapshot) {
    return `${screen.view}:${screen.patientId ?? ''}:${screen.patientTab}`;
  }
  function scrollToScreenStart() {
    window.scrollTo({ top: 0, left: 0, behavior: 'instant' });
  }
  $effect(() => {
    const nextScreen = getScreenSnapshot();
    const nextScreenKey = screenKey(nextScreen);
    if (nextScreenKey === lastScreenKey) return;
    if (currentScreen && !restoringNavigation) {
      navigationHistory.push(currentScreen);
      if (navigationHistory.length > 40) navigationHistory.shift();
    }
    currentScreen = nextScreen;
    lastScreenKey = nextScreenKey;
    void tick().then(() => {
      if (lastScreenKey === nextScreenKey) scrollToScreenStart();
    });
  });
  async function resetNavigation() {
    restoringNavigation = true;
    navigationHistory = [];
    view = 'home';
    patientTab = 'overview';
    await tick();
    currentScreen = getScreenSnapshot();
    lastScreenKey = screenKey(currentScreen);
    restoringNavigation = false;
    scrollToScreenStart();
  }
  async function handleMobileBack() {
    const dialogs = document.querySelectorAll<HTMLDialogElement>('dialog[open]');
    const openDialog = dialogs.item(dialogs.length - 1);
    if (openDialog) {
      openDialog.dispatchEvent(new Event('cancel', { cancelable: true }));
      return;
    }
    if (manager) {
      manager = null;
      return;
    }
    if (modal) {
      modal = null;
      return;
    }
    const previous = navigationHistory.pop();
    if (previous) {
      restoringNavigation = true;
      view = previous.view;
      if (previous.patientId && selected?.id !== previous.patientId)
        await loadPatient(previous.patientId);
      else if (previous.view === 'messages' && !previous.patientId) selected = null;
      patientTab = previous.patientTab;
      await tick();
      currentScreen = getScreenSnapshot();
      lastScreenKey = screenKey(currentScreen);
      restoringNavigation = false;
      scrollToScreenStart();
      return;
    }
    if (view !== 'home') {
      await resetNavigation();
      return;
    }
    await exit(0);
  }
  function openDialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!saving) modal = null;
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
  function fail(cause: unknown) {
    error = cause instanceof Error ? cause.message : 'Не удалось выполнить действие';
    if (cause instanceof ApiError && cause.status === 401) {
      user = null;
      patients = [];
      patientTotal = 0;
      selected = null;
      entries = [];
      events = [];
      recentEvents = [];
      changes = [];
      changesCursor = null;
      changesMoreBusy = false;
    }
  }
  async function loadPatient(id: string) {
    if (selected?.id !== id) patientTab = 'overview';
    const current = ++generation;
    ++eventRequest;
    ++diaryRequest;
    diaryMoreBusy = false;
    eventMoreBusy = false;
    eventPageFailed = false;
    changesMoreBusy = false;
    const filter = selected?.id === id ? eventFilter : 'all';
    loading = true;
    try {
      const [profile, diary, timeline, revisions, filteredTimeline] = await Promise.all([
        api('get_patient', {
          params: { id: id },
        }),
        api('list_diary_page', {
          params: { id: id },
          query: {
            limit: 365,
          },
        }),
        api('list_events_page', {
          params: { id: id },
          query: {
            kind: 'all',
          },
        }),
        user?.role === 'doctor'
          ? api('list_profile_changes_page', {
              params: { id: id },
            })
          : Promise.resolve({ items: [], next_cursor: null }),
        filter !== 'all'
          ? api('list_events_page', {
              params: { id: id },
              query: {
                kind: filter,
              },
            })
          : Promise.resolve(null),
      ]);
      if (current === generation) {
        if (selected?.id !== profile.id) eventFilter = 'all';
        selected = profile;
        patients = patients.some((patient) => patient.id === profile.id)
          ? patients.map((patient) => (patient.id === profile.id ? profile : patient))
          : [...patients, profile];
        entries = diary.items;
        recentEvents = timeline.items;
        events = (filteredTimeline ?? timeline).items;
        diaryCursor = diary.next_cursor;
        eventCursor = (filteredTimeline ?? timeline).next_cursor;
        changes = revisions.items;
        changesCursor = revisions.next_cursor;
        return true;
      }
    } catch (cause) {
      if (current === generation) fail(cause);
    } finally {
      if (current === generation) loading = false;
    }
    return false;
  }
  async function loadMoreChanges() {
    if (!selected || !changesCursor || changesMoreBusy || loading || !doctor) return;
    const current = generation,
      id = selected.id;
    changesMoreBusy = true;
    error = '';
    try {
      const page = await api('list_profile_changes_page', {
        params: { id: id },
        query: {
          before_version: changesCursor.before_version,
        },
      });
      if (current !== generation || !user) return;
      changes = [
        ...changes,
        ...page.items.filter((item) => !changes.some((existing) => existing.id === item.id)),
      ];
      changesCursor = page.next_cursor;
    } catch (cause) {
      if (current === generation) fail(cause);
    } finally {
      if (current === generation) changesMoreBusy = false;
    }
  }
  async function loadMoreDiary() {
    if (!selected || !diaryCursor || diaryMoreBusy || loading) return;
    const current = generation,
      request = ++diaryRequest,
      id = selected.id;
    diaryMoreBusy = true;
    error = '';
    try {
      const page = await api('list_diary_page', {
        params: { id: id },
        query: { ...diaryCursor, limit: 50 },
      });
      if (current !== generation || request !== diaryRequest || !user) return;
      entries = [
        ...entries,
        ...page.items.filter((item) => !entries.some((existing) => existing.id === item.id)),
      ].sort(
        (a, b) =>
          Date.parse(b.measured_at) - Date.parse(a.measured_at) ||
          Date.parse(b.created_at) - Date.parse(a.created_at) ||
          b.id.localeCompare(a.id),
      );
      diaryCursor = page.next_cursor;
    } catch (cause) {
      if (current === generation && request === diaryRequest) fail(cause);
    } finally {
      if (current === generation && request === diaryRequest) diaryMoreBusy = false;
    }
  }
  async function loadEvents(reset = false) {
    if (!selected || loading || (!reset && (!eventCursor || eventMoreBusy))) return;
    const current = generation,
      request = ++eventRequest,
      id = selected.id,
      filter = eventFilter;
    eventMoreBusy = true;
    eventPageFailed = false;
    eventRetryReset = reset;
    error = '';
    if (reset) {
      events = [];
      eventCursor = null;
    }
    try {
      const page = await api('list_events_page', {
        params: { id: id },
        query: { kind: filter, ...(!reset && eventCursor ? eventCursor : {}) },
      });
      if (current !== generation || request !== eventRequest || !user) return;
      events = reset
        ? page.items
        : [
            ...events,
            ...page.items.filter((item) => !events.some((existing) => existing.id === item.id)),
          ].sort(
            (a, b) =>
              Date.parse(b.occurred_at) - Date.parse(a.occurred_at) ||
              Date.parse(b.created_at) - Date.parse(a.created_at) ||
              b.id.localeCompare(a.id),
          );
      eventCursor = page.next_cursor;
    } catch (cause) {
      if (current === generation && request === eventRequest) {
        eventPageFailed = true;
        fail(cause);
      }
    } finally {
      if (current === generation && request === eventRequest) eventMoreBusy = false;
    }
  }
  async function refresh() {
    if (user?.role === 'admin') return;
    if (refreshing) return;
    error = '';
    refreshing = true;
    try {
      const [directory] = await Promise.all([
        api('list_patients_page', {
          query: { limit: 50 },
        }),
        notificationBell?.refreshNotifications(),
      ]);
      const list = directory.items;
      patients = list;
      patientTotal = directory.total_count;
      directoryRefreshKey++;
      const id = selected?.id ?? (!doctor ? list[0]?.id : undefined);
      if (id) await loadPatient(id);
      else {
        selected = null;
        entries = [];
        events = [];
        recentEvents = [];
        changes = [];
        changesCursor = null;
        changesMoreBusy = false;
      }
    } catch (cause) {
      fail(cause);
    } finally {
      refreshing = false;
    }
  }
  function navigateToSection(nextView: string) {
    const sameScreen = view === nextView && !(doctor && nextView === 'messages' && selected);
    if (doctor && nextView === 'messages') selected = null;
    view = nextView;
    error = '';
    if (sameScreen) void tick().then(scrollToScreenStart);
  }
  function handleHomeView(nextView: string) {
    if (doctor && selected && nextView === 'appointments') {
      patientTab = 'treatment';
      view = 'observation';
      return;
    }
    view = nextView;
  }
  async function openNotification(notification: Notification) {
    error = '';
    eventFilter = 'all';
    if (!(await loadPatient(notification.patient_id)))
      throw new Error(error || 'Не удалось открыть историю пациента');
    const current = generation;
    if (
      notification.kind === 'prescription_published' ||
      notification.kind === 'adverse_event' ||
      notification.kind === 'critical_alert' ||
      notification.kind === 'chat_message' ||
      notification.kind === 'appointment' ||
      notification.kind.startsWith('appointment_')
    ) {
      directoryRefreshKey++;
      if (doctor) {
        if (notification.kind === 'chat_message') {
          view = 'messages';
          await tick();
          return;
        }
        patientTab =
          notification.kind === 'appointment' || notification.kind.startsWith('appointment_')
            ? 'treatment'
            : notification.kind === 'critical_alert'
              ? 'overview'
              : 'treatment';
        view = 'observation';
        await tick();
        if (
          notification.kind === 'critical_alert' ||
          notification.kind === 'prescription_published' ||
          notification.kind === 'adverse_event'
        ) {
          document
            .getElementById(
              notification.kind === 'critical_alert'
                ? `alert-${notification.related_id}`
                : `therapy-${notification.related_id}`,
            )
            ?.scrollIntoView({ block: 'center' });
        }
        return;
      }
      view =
        notification.kind === 'chat_message'
          ? 'messages'
          : notification.kind === 'appointment' || notification.kind.startsWith('appointment_')
            ? 'appointments'
            : 'profile';
      await tick();
      if (
        notification.kind !== 'chat_message' &&
        notification.kind !== 'appointment' &&
        !notification.kind.startsWith('appointment_')
      ) {
        document
          .getElementById(
            notification.kind === 'critical_alert'
              ? `alert-${notification.related_id}`
              : `therapy-${notification.related_id}`,
          )
          ?.scrollIntoView({ block: 'center' });
      }
      return;
    }
    if (
      notification.kind === 'diary_entry' &&
      !entries.some((entry) => entry.id === notification.related_id)
    ) {
      const entry = await api('get_diary_entry', {
        params: { id: notification.patient_id, entry_id: notification.related_id },
      });
      if (current !== generation)
        throw new Error('Выбран другой пациент. Повторите открытие записи.');
      entries = [...entries, entry].sort(
        (a, b) => Date.parse(b.measured_at) - Date.parse(a.measured_at),
      );
    }
    if (
      notification.kind === 'published_event' &&
      !events.some((event) => event.id === notification.related_id)
    ) {
      const event = await api('get_event', {
        params: { id: notification.patient_id, event_id: notification.related_id },
      });
      if (current !== generation)
        throw new Error('Выбран другой пациент. Повторите открытие записи.');
      events = [...events, event].sort(
        (a, b) => Date.parse(b.occurred_at) - Date.parse(a.occurred_at),
      );
    }
    eventFilter = 'all';
    if (doctor) patientTab = notification.kind === 'diary_entry' ? 'observation' : 'history';
    view = doctor ? 'observation' : 'history';
    await tick();
    document
      .getElementById(`record-${notification.related_id}`)
      ?.scrollIntoView({ block: 'center' });
  }
  onMount(() => {
    let mounted = true;
    if (isTauri()) {
      void onBackButtonPress(() => void handleMobileBack())
        .then((listener) => {
          if (mounted) backButtonListener = listener;
          else void listener.unregister();
        })
        .catch(fail);
    }
    const openInvitation = () => {
      if (!new URLSearchParams(location.hash.slice(1)).has('invite')) return;
      generation++;
      authInvite = location.hash;
      user = null;
      selected = null;
      patients = [];
      entries = [];
      events = [];
      recentEvents = [];
      changes = [];
      manager = null;
      modal = null;
      booting = false;
      void resetNavigation();
    };
    window.addEventListener('hashchange', openInvitation);
    (async () => {
      if (new URLSearchParams(location.hash.slice(1)).has('invite')) {
        booting = false;
        return;
      }
      try {
        const current = generation;
        const authenticated = await api('me');
        if (current !== generation || new URLSearchParams(location.hash.slice(1)).has('invite'))
          return;
        user = authenticated;
        await refresh();
        await resetNavigation();
      } catch (cause) {
        if (!(cause instanceof ApiError && cause.status === 401)) fail(cause);
      } finally {
        booting = false;
      }
    })();
    return () => {
      mounted = false;
      window.removeEventListener('hashchange', openInvitation);
      if (backButtonListener) {
        void backButtonListener.unregister();
        backButtonListener = null;
      }
    };
  });
  async function logout() {
    error = '';
    try {
      await api('logout');
      generation++;
      user = null;
      selected = null;
      patients = [];
      patientTotal = 0;
      entries = [];
      events = [];
      recentEvents = [];
      changes = [];
      changesCursor = null;
      changesMoreBusy = false;
      notice = '';
      modal = null;
      manager = null;
      await resetNavigation();
    } catch (cause) {
      fail(cause);
    }
  }
  function openManager(mode: 'create' | 'edit' | 'invite') {
    managerTarget = mode === 'create' || !selected ? null : { ...selected };
    manager = mode;
    error = '';
    notice = '';
  }
  async function profileSaved(profile: Patient) {
    const directory = await api('list_patients_page', {
      query: { limit: 50 },
    });
    patients = directory.items;
    patientTotal = directory.total_count;
    directoryRefreshKey++;
    await loadPatient(profile.id);
    view = doctor ? 'observation' : 'profile';
    if (doctor) patientTab = 'overview';
    notice = 'Карточка пациента сохранена.';
  }
  function openModal(kind: 'diary' | 'note') {
    submissionId = crypto.randomUUID();
    modal = kind;
    error = '';
    notice = '';
    measuredAt = localDateTime();
    comment = '';
    noteTitle = '';
    noteBody = '';
    patientVisible = false;
    noteKind = 'note';
    noteSource = '';
    noteOccurredAt = localDateTime();
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!selected || saving) return;
    const id = selected.id;
    saving = true;
    error = '';
    try {
      if (modal === 'diary') {
        await api('save_diary', {
          params: { id: id },
          body: {
            id: submissionId,
            measured_at: new Date(measuredAt).toISOString(),
            mood: Number(mood),
            anxiety: Number(anxiety),
            sleep_hours: Number(sleep),
            comment,
          },
        });
        notice = 'Запись сохранена. Она доступна вашему лечащему врачу.';
      } else {
        await api('save_event', {
          params: { id: id },
          body: {
            id: submissionId,
            title: noteTitle,
            body: noteBody,
            patient_visible: patientVisible,
            kind: noteKind,
            source: noteSource.trim() || null,
            occurred_at: eventDateTime(noteOccurredAt),
          },
        });
        notice = patientVisible
          ? 'Запись сохранена и доступна пациенту.'
          : 'Внутренняя заметка сохранена. Пациент её не видит.';
      }
      modal = null;
      await refresh();
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }
</script>

{#if booting}
  <div class="boot">
    <LoaderCircle class="spin" size={28} /><span>Открываем ваш кабинет</span>
  </div>
{:else if !user}
  {#key authInvite}
    <AuthPanel
      onauthenticated={async (value) => {
        user = value;
        error = '';
        view = 'home';
        await refresh();
      }}
    />
  {/key}
{:else if user.role === 'admin'}
  <AdminPanel onlogout={logout} />
{:else}
  <div
    class="app-shell"
    class:patient-workspace={!doctor}
    class:messages-view={view === 'messages'}
    use:pullToRefresh={{
      disabled: loading || refreshing || !!modal || !!manager,
      onrefresh: refresh,
      onchange: (distance, busy, phase) => {
        pullDistance = distance;
        gestureBusy = busy;
        pullPhase = phase;
      },
    }}
  >
    {#if pullPhase !== 'idle' && pullDistance >= PULL_INDICATOR_THRESHOLD}<div
        class="pull-indicator"
        class:returning={pullPhase === 'returning'}
        style:opacity={Math.min(1, pullDistance / 32)}
        style:transform={`translate(-50%, ${Math.min(0, pullDistance / 4 - 8)}px)`}
        role="status"
        aria-live="polite"
      >
        <RefreshCw size={18} class={gestureBusy ? 'spin' : ''} /><span
          >{gestureBusy
            ? 'Обновляем'
            : pullDistance >= PULL_REFRESH_THRESHOLD
              ? 'Отпустите для обновления'
              : 'Потяните вниз'}</span
        >
      </div>{/if}
    <aside class="sidebar">
      <Brand size={40} />
      <div class="workspace-label">{doctor ? 'КАБИНЕТ ВРАЧА' : 'КАБИНЕТ ПАЦИЕНТА'}</div>
      <nav aria-label="Основные разделы">
        {#each patientNavigation as item}<button
            class:active={activeNavigation === item.id}
            aria-current={activeNavigation === item.id ? 'page' : undefined}
            onclick={() => navigateToSection(item.id)}
            ><item.icon size={22} />{item.label}{#if item.id === 'patients'}<span class="nav-count"
                >{patientTotal}</span
              >{:else if item.id === 'messages' && unreadMessages}<span class="nav-count nav-unread"
                >{unreadMessages > 99 ? '99+' : unreadMessages}</span
              >{/if}</button
          >{/each}
      </nav>
      <div class="sidebar-bottom">
        <div class="care-note">
          <strong>Мы рядом</strong>
          <p>Точка связи. Пространство заботы.</p>
        </div>
      </div>
    </aside>
    <div class="workspace">
      <header class="topbar">
        <div class="mobile-brand"><Brand /></div>
        <div class="topbar-right">
          <span class="demo-tag">Демо</span><NotificationBell
            {doctor}
            onopen={openNotification}
            onmessageschange={(count) => (unreadMessages = count)}
            bind:this={notificationBell}
          />
          <button
            class="avatar-button"
            aria-label={doctor ? 'Открыть медкарту пациента' : 'Открыть профиль'}
            onclick={() => {
              if (doctor) {
                if (selected) {
                  patientTab = 'overview';
                  view = 'observation';
                } else view = 'patients';
              } else view = 'profile';
            }}><Avatar name={user.name} /></button
          >
          <span class="user-name">{user.name}<small>{doctor ? 'Врач' : 'Пациент'}</small></span
          ><button class="icon-button topbar-logout" aria-label="Выйти из аккаунта" onclick={logout}
            ><LogOut size={18} /></button
          >
        </div>
      </header>
      <main
        class="page-content"
        class:messages-page={view === 'messages'}
        class:is-pulling={pullPhase !== 'idle'}
        style:transform={pullDistance ? `translateY(${pullDistance / 2}px)` : undefined}
      >
        <h1 class="visually-hidden">{pageTitle}</h1>
        {#if selected && !doctor && (view === 'dynamics' || view === 'history' || view === 'observation' || view === 'profile')}
          <div class="page-actions">
            <button class="primary" disabled={loading} onclick={() => openModal('diary')}
              ><Plus size={18} />Как я себя чувствую</button
            >
          </div>
        {/if}
        {#if error && !modal}<div class="feedback error" role="alert">
            {error}<button class="text-button" onclick={refresh}>Повторить</button>
          </div>{/if}
        {#if notice}<div class="feedback success" role="status">
            <Check size={18} />{notice}<button
              class="icon-button"
              aria-label="Закрыть сообщение"
              onclick={() => (notice = '')}><X size={16} /></button
            >
          </div>{/if}

        {#if view === 'patients'}
          <PatientDirectory
            refreshKey={directoryRefreshKey}
            {selected}
            onselect={async (id) => {
              const loaded = await loadPatient(id);
              if (loaded) {
                patientTab = 'overview';
                view = 'observation';
              }
              return loaded;
            }}
            oncreate={() => openManager('create')}
          />
        {:else if !selected && (refreshing || loading)}
          <section class="card empty-state" role="status">
            <LoaderCircle class="spin" size={28} />
            <p>Загружаем историю наблюдения</p>
          </section>
        {:else if !selected && view === 'appointments'}
          <AppointmentsPanel
            {doctor}
            patient={selected}
            refreshKey={directoryRefreshKey}
            onerror={fail}
          />
        {:else if !selected && doctor && view === 'messages'}
          <ChatThreads
            onselect={async (id) => {
              const loaded = await loadPatient(id);
              if (loaded) view = 'messages';
              return loaded;
            }}
            onerror={fail}
          />
        {:else if !selected && !doctor && view === 'profile'}
          <section class="card account-profile">
            <div class="profile-top">
              <Avatar name={user.name} size={72} />
              <div>
                <h2>{user.name}</h2>
                <p class="muted">Аккаунт пациента</p>
              </div>
            </div>
            <dl>
              <dt>Телефон или почта</dt>
              <dd>{user.email}</dd>
            </dl>
            <p class="muted">
              Наблюдение ещё не начато. Примите приглашение врача, чтобы открыть медицинскую карту.
            </p>
          </section>
        {:else if !selected && view === 'home' && doctor}
          <HomePanel
            refreshKey={directoryRefreshKey}
            onerror={fail}
            {user}
            patient={null}
            {entries}
            onview={handleHomeView}
            ondiary={() => openModal('diary')}
            oncreate={() => openManager('create')}
            onpatient={async (id) => {
              await loadPatient(id);
              patientTab = 'overview';
              view = 'observation';
            }}
          />
        {:else if !selected && doctor && view === 'observation'}
          <section class="card empty-state">
            <Activity size={32} />
            <h2>Выберите пациента для наблюдения</h2>
            <p>
              Откройте раздел «Пациенты», чтобы посмотреть динамику, приёмы и историю в одном
              рабочем окне.
            </p>
            <button class="primary" onclick={() => (view = 'patients')}
              ><Users size={18} />Открыть пациентов</button
            >
          </section>
        {:else if !selected}
          {#if view === 'home'}<Greeting {user} />{/if}
          <section class="card empty-state">
            <Users size={32} />
            <h2>Наблюдение ещё не начато</h2>
            <p>
              {doctor
                ? 'В этом аккаунте пока нет пациентов для наблюдения.'
                : 'После связи с лечащим врачом здесь появится ваша история.'}
            </p>
          </section>
        {:else}
          {#if doctor && view !== 'home'}<div class="patient-context">
              <div class="patient-context-left">
                <button
                  class="text-link patient-directory-back"
                  onclick={() => {
                    selected = null;
                    view = 'patients';
                  }}><ArrowRight size={16} class="chat-back-icon" />Все пациенты</button
                >
                <div class="patient-switcher">
                  <span class="muted">История пациента</span><select
                    aria-label="Выбрать пациента"
                    value={selected.id}
                    disabled={loading}
                    onchange={(event) => loadPatient(event.currentTarget.value)}
                    >{#each patients as patient}<option value={patient.id}>{patient.name}</option
                      >{/each}</select
                  >
                </div>
              </div>
              {#if view === 'dynamics' || view === 'history' || view === 'observation' || view === 'profile'}
                <button
                  class="primary patient-note-action"
                  disabled={loading}
                  onclick={() => openModal('note')}><Plus size={18} />Добавить заметку</button
                >
              {/if}
            </div>{/if}
          {#if loading}<div class="loading-strip" role="status">
              <LoaderCircle class="spin" size={16} /> Обновляем историю
            </div>{/if}
          {#if doctor && selected && view !== 'home' && view !== 'patients' && view !== 'messages'}
            <DoctorPatientWorkspace
              patient={selected}
              refreshKey={directoryRefreshKey}
              {entries}
              {diaryCursor}
              {diaryMoreBusy}
              {events}
              {eventCursor}
              {eventMoreBusy}
              {eventPageFailed}
              {changes}
              {changesCursor}
              {changesMoreBusy}
              {loading}
              bind:tab={patientTab}
              bind:eventFilter
              onedit={() => openManager('edit')}
              oninvite={() => openManager('invite')}
              onerror={fail}
              onloadDiary={loadMoreDiary}
              onloadEvents={() => loadEvents()}
              onretryEvents={() => loadEvents(true)}
              onloadChanges={loadMoreChanges}
            />
          {:else}<div class:pending={loading} class:chat-page-layout={view === 'messages'}>
              {#if (doctor && view === 'profile') || view === 'dynamics'}
                <QuestionnairePanel
                  patient={selected}
                  {doctor}
                  refreshKey={directoryRefreshKey}
                  showHistory={view === 'dynamics'}
                  onerror={fail}
                />
              {/if}
              {#if view === 'home'}
                <HomePanel
                  refreshKey={directoryRefreshKey}
                  onerror={fail}
                  {user}
                  patient={selected}
                  {entries}
                  onview={handleHomeView}
                  ondiary={() => openModal('diary')}
                  oncreate={() => openManager('create')}
                  onpatient={async (id) => {
                    await loadPatient(id);
                    patientTab = 'overview';
                    view = 'observation';
                  }}
                />
              {/if}
              {#if view === 'profile'}
                <div class="medical-page" class:patient-medical={!doctor}>
                  <div class="medical-layout">
                    {#if doctor}
                      <section class="card profile-card">
                        <div class="profile-top">
                          <Avatar name={selected.name} size={72} />
                          <div>
                            <span class="eyebrow">{doctor ? 'ПАЦИЕНТ' : 'МОЯ КАРТОЧКА'}</span>
                            <h2>{selected.name}</h2>
                            <span
                              class="status"
                              class:observation={selected.status === 'observation'}
                              >{statusLabel(selected.status)}</span
                            >
                          </div>
                        </div>
                        <div class="profile-details">
                          <span
                            >Начало наблюдения<strong>{date(selected.observation_started)}</strong
                            ></span
                          ><span>Лечащий врач<strong>{selected.doctor_name}</strong></span><span
                            >Аллергии<strong>{selected.allergies ?? 'Нет сведений'}</strong></span
                          >
                        </div>
                        <p class="profile-summary">
                          {selected.medical_summary || 'Медицинские сведения пока не внесены.'}
                        </p>
                        {#if doctor}<div class="profile-actions">
                            <button class="secondary" onclick={() => openManager('edit')}
                              >Редактировать карточку</button
                            >{#if !selected.account_activated}<button
                                class="secondary"
                                onclick={() => openManager('invite')}>Подключить пациента</button
                              >{/if}
                          </div>{/if}
                        {#if doctor && !selected.account_activated}<p
                            class="muted small-text connection-state"
                          >
                            Пациент ещё не принял приглашение. Передайте ему ссылку, чтобы открыть
                            доступ к дневнику и истории.
                          </p>{/if}
                      </section>
                    {:else}<BasicInformation patient={selected} />{/if}
                    <section class="card profile-observation">
                      <h2>История наблюдения</h2>
                      <p class="muted medical-description">
                        Наблюдение начато {date(selected.observation_started)}.<br
                        />{selected.last_entry_at
                          ? `Последняя запись: ${date(selected.last_entry_at, true)}`
                          : 'Записей дневника пока нет.'}
                      </p>
                      {#if !doctor}
                        <div class="treating-doctor">
                          <Avatar name={selected.doctor_name} size={42} />
                          <div>
                            <h3>Лечащий врач</h3>
                            <p class="muted">{selected.doctor_name}</p>
                          </div>
                          <button class="secondary" onclick={() => navigateToSection('messages')}
                            >Написать</button
                          >
                        </div>
                      {/if}
                      <button
                        class="secondary full-width"
                        onclick={() => (view = doctor ? 'observation' : 'dynamics')}
                        >Рубежи и показатели</button
                      ><button
                        class="text-link"
                        onclick={() => (view = doctor ? 'observation' : 'history')}
                        >История наблюдения<ArrowRight size={16} /></button
                      >
                    </section>
                  </div>
                  <AlertPanel
                    patient={selected}
                    {doctor}
                    refreshKey={directoryRefreshKey}
                    onerror={fail}
                  />
                  <TherapyPanel
                    patient={selected}
                    {doctor}
                    refreshKey={directoryRefreshKey}
                    onerror={fail}
                  />
                  {#if !doctor}<ClinicalRecommendationsPanel
                      patient={selected}
                      doctor={false}
                      refreshKey={directoryRefreshKey}
                      onerror={fail}
                    />{/if}
                  <MedicalHistory history={selected.medical_history} />
                  <ClinicalPanel
                    patient={selected}
                    {doctor}
                    refreshKey={directoryRefreshKey}
                    onerror={fail}
                  />
                </div>
              {/if}

              {#if view === 'messages'}
                {#if doctor && !selected}<ChatThreads
                    onselect={async (id) => {
                      const loaded = await loadPatient(id);
                      if (loaded) view = 'messages';
                      return loaded;
                    }}
                    onerror={fail}
                  />{/if}
                {#if doctor && selected}<button
                    class="text-link chat-back"
                    onclick={() => {
                      selected = null;
                      view = 'messages';
                    }}><ArrowRight size={16} class="chat-back-icon" />Все диалоги</button
                  >{/if}
                <ChatPanel
                  patient={selected}
                  {doctor}
                  userId={user.id}
                  refreshKey={directoryRefreshKey}
                  onerror={fail}
                />
              {/if}
              {#if view === 'medications'}
                <TherapyPanel
                  patient={selected}
                  {doctor}
                  refreshKey={directoryRefreshKey}
                  onerror={fail}
                />
              {/if}
              {#if view === 'appointments' || (doctor && view === 'observation')}
                <AppointmentsPanel
                  {doctor}
                  patient={selected}
                  refreshKey={directoryRefreshKey}
                  onerror={fail}
                />
              {/if}
              {#if doctor && view === 'observation'}
                <TherapyPanel
                  patient={selected}
                  {doctor}
                  refreshKey={directoryRefreshKey}
                  onerror={fail}
                />
              {/if}

              {#if view === 'dynamics' || (doctor && view === 'observation')}
                <div class="detail-grid">
                  <section class="card dynamics-card">
                    <div class="section-heading">
                      <div>
                        <h2>{doctor ? 'Динамика пациента' : 'Моя динамика'}</h2>
                        <p class="muted">Для каждого показателя своя шкала</p>
                      </div>
                      <div class="period-selector" role="group" aria-label="Период графика">
                        <button
                          class:active={period === 7}
                          aria-pressed={period === 7}
                          onclick={() => (period = 7)}
                        >
                          7 дней
                        </button>
                        <button
                          class:active={period === 30}
                          aria-pressed={period === 30}
                          onclick={() => (period = 30)}
                        >
                          30 дней
                        </button>
                        <button
                          class:active={period === 90}
                          aria-pressed={period === 90}
                          onclick={() => (period = 90)}
                        >
                          3 месяца
                        </button>
                      </div>
                    </div>
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
                    <p class="chart-footnote">
                      Дневниковые самооценки. Не являются диагнозом или результатами клинической
                      шкалы.
                      {#if diaryCursor}Графики построены по загруженным записям; более ранние
                        доступны ниже.{/if}
                    </p>
                  </section>
                  {#if view !== 'observation'}<section class="card timeline-card">
                      <div class="section-heading">
                        <div>
                          <h2>События наблюдения</h2>
                          <p class="muted">История важных встреч и записей</p>
                        </div>
                        <CalendarDays size={19} />
                      </div>
                      <EventTimeline events={recentEvents.slice(0, 4)} {doctor} />
                      <button
                        class="text-link"
                        onclick={() => (view = doctor ? 'observation' : 'history')}
                        >Вся история<ArrowRight size={16} /></button
                      >
                    </section>{/if}
                </div>
              {/if}

              {#if view === 'history' || (doctor && view === 'observation')}
                <section class="card">
                  <div class="section-heading">
                    <div>
                      <h2>События и заметки</h2>
                      <p class="muted">
                        {doctor
                          ? 'Внутренние записи видите только вы'
                          : 'Записи, которыми с вами поделился врач'}
                      </p>
                    </div>
                  </div>
                  <div class="event-filter">
                    <label for="event-filter">Фильтр по типу события</label>
                    <select
                      id="event-filter"
                      bind:value={eventFilter}
                      disabled={loading}
                      onchange={() => loadEvents(true)}
                      ><option value="all">Все события</option>
                      {#each Object.entries(eventKinds) as [value, label]}<option {value}
                          >{label}</option
                        >{/each}
                    </select>
                  </div>
                  {#if filteredEvents.length || (!eventMoreBusy && !eventPageFailed)}<EventTimeline
                      events={filteredEvents}
                      {doctor}
                    />{/if}
                  {#if eventMoreBusy}<p role="status" class="muted">Загружаем события</p>{/if}
                  {#if eventPageFailed}<button
                      class="secondary"
                      onclick={() => loadEvents(eventRetryReset)}>Повторить загрузку событий</button
                    >
                  {:else if eventCursor}<button
                      class="secondary"
                      disabled={eventMoreBusy || loading}
                      onclick={() => loadEvents()}>Загрузить ещё события</button
                    >{/if}
                </section>
                {#if doctor}<ProfileHistory
                    {changes}
                    hasMore={changesCursor !== null}
                    busy={changesMoreBusy || loading}
                    onmore={loadMoreChanges}
                  />{/if}
              {/if}

              {#if view === 'history' || view === 'dynamics' || (doctor && view === 'observation')}
                <section class="card diary-history">
                  <div class="section-heading">
                    <div>
                      <h2>Дневник показателей</h2>
                      <p class="muted">Последние записи пациента ({entries.length})</p>
                    </div>
                    <ClipboardList size={20} />
                  </div>
                  {#each entries as entry}<article class="diary-row" id={`record-${entry.id}`}>
                      <div>
                        <strong>{date(entry.measured_at, true)}</strong><span class="diary-values"
                          >Настроение {entry.mood}/10, Тревога {entry.anxiety}/10,
                          Сон {entry.sleep_hours} ч</span
                        >
                      </div>
                      {#if entry.comment}<p>{entry.comment}</p>{/if}
                    </article>{:else}<div class="empty-state">
                      Первая запись появится после заполнения дневника.
                    </div>{/each}
                  {#if diaryCursor}<button
                      class="secondary"
                      disabled={diaryMoreBusy || loading}
                      onclick={loadMoreDiary}
                      >{diaryMoreBusy
                        ? 'Загружаем дневник'
                        : 'Загрузить ещё записи дневника'}</button
                    >{/if}
                </section>
              {/if}
            </div>{/if}
        {/if}
        <footer class="page-footer">
          <span>СИНАПС: точка связи</span><span>Демонстрационный стенд. Синтетические данные</span
          >
        </footer>
      </main>
    </div>
    <nav class="mobile-nav" aria-label="Мобильная навигация">
      {#each patientNavigation as item}<button
          class:active={activeNavigation === item.id}
          aria-current={activeNavigation === item.id ? 'page' : undefined}
          onclick={() => navigateToSection(item.id)}
          >{#if item.asset}<img
              class="nav-icon"
              src={`/design/${item.asset}.svg`}
              alt=""
            />{:else}<item.icon size={21} />{/if}<span>{item.label}</span
          >{#if item.id === 'messages' && unreadMessages}<span class="nav-count nav-unread"
              >{unreadMessages > 99 ? '99+' : unreadMessages}</span
            >{/if}</button
        >{/each}
    </nav>
  </div>
{/if}

{#if manager && user && doctor}
  <PatientManager
    mode={manager}
    patient={managerTarget}
    onclose={() => (manager = null)}
    onsaved={profileSaved}
    onerror={fail}
  />
{/if}

{#if modal && user && selected}
  <dialog class="modal card" use:openDialog aria-labelledby="modal-title">
    <div class="section-heading">
      <div>
        <span class="eyebrow">{modal === 'diary' ? 'МОЙ ДНЕВНИК' : selected.name}</span>
        <h2 id="modal-title">
          {modal === 'diary' ? 'Как вы себя чувствуете?' : 'Новая запись наблюдения'}
        </h2>
      </div>
      <button
        class="icon-button"
        aria-label="Закрыть"
        disabled={saving}
        onclick={() => (modal = null)}><X size={21} /></button
      >
    </div>
    <form onsubmit={save}>
      {#if modal === 'diary'}
        <p class="muted form-intro">
          Выберите то, что ближе к вашим ощущениям. Запись будет доступна вашему врачу.
        </p>
        <label for="measured">Дата и время наблюдения</label><input
          id="measured"
          type="datetime-local"
          bind:value={measuredAt}
          max={localDateTime()}
          required
        />
        <label class="range-label" for="mood">Настроение <strong>{mood} / 10</strong></label><input
          id="mood"
          type="range"
          min="0"
          max="10"
          step="1"
          bind:value={mood}
        />
        <div class="range-hints"><span>Очень низкое</span><span>Очень хорошее</span></div>
        <label class="range-label" for="anxiety">Тревога <strong>{anxiety} / 10</strong></label
        ><input
          id="anxiety"
          class="anxiety-range"
          type="range"
          min="0"
          max="10"
          step="1"
          bind:value={anxiety}
        />
        <div class="range-hints"><span>Нет тревоги</span><span>Очень сильная</span></div>
        <label for="sleep">Продолжительность сна, часов</label><input
          id="sleep"
          type="number"
          min="0"
          max="24"
          step="0.5"
          bind:value={sleep}
          required
        />
        <label for="comment">Что ещё важно отметить? <span class="muted">Необязательно</span></label
        ><textarea
          id="comment"
          rows="3"
          maxlength="2000"
          placeholder="Опишите, как прошёл день и что повлияло на самочувствие"
          bind:value={comment}></textarea>
      {:else}
        <label for="note-kind">Тип события</label><select id="note-kind" bind:value={noteKind}>
          {#each Object.entries(eventKinds) as [value, label]}<option {value}>{label}</option
            >{/each}
        </select>
        <label for="note-occurred-at">Дата и время события</label><input
          id="note-occurred-at"
          type="datetime-local"
          bind:value={noteOccurredAt}
          min={`${selected.birth_date}T00:00`}
          max={localDateTime()}
          required
        />
        <p class="muted small-text">
          Для прошлых событий укажите фактическую дату. Время внесения и автор сохранятся отдельно.
        </p>
        <label for="note-title">Заголовок</label><input
          id="note-title"
          bind:value={noteTitle}
          maxlength="120"
          placeholder="Например, повторная консультация"
          required
        />
        <label for="note-body">Запись врача</label><textarea
          id="note-body"
          rows="6"
          bind:value={noteBody}
          maxlength="4000"
          placeholder="Наблюдения и вопросы для следующей встречи"
          required></textarea>
        <label for="note-source">Источник сведений <span class="muted">Необязательно</span></label
        ><input
          id="note-source"
          bind:value={noteSource}
          maxlength="500"
          placeholder="Например, выписка от 12.03.2026 или со слов пациента"
        />
        <label class="checkbox"
          ><input type="checkbox" bind:checked={patientVisible} />Показывать эту запись пациенту</label
        >
        <p class="muted small-text">
          Без отметки запись останется внутренней. При публикации пациенту будут видны текст и
          источник. Здесь не создаются назначения лекарств.
        </p>
      {/if}
      {#if error}<div class="feedback error" role="alert">{error}</div>{/if}
      <div class="modal-actions">
        <button class="secondary" type="button" disabled={saving} onclick={() => (modal = null)}
          >Отмена</button
        ><button class="primary" type="submit" disabled={saving}
          >{saving ? 'Сохраняем' : 'Сохранить запись'}<Check size={17} /></button
        >
      </div>
    </form>
  </dialog>
{/if}
