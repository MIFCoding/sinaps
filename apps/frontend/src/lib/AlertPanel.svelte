<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { AlertTriangle, CheckCircle2, ShieldAlert } from '@lucide/svelte';
  import { api, date, type Alert, type AlertPage, type Patient } from './api';

  let {
    doctor,
    patient = null,
    activeOnly = false,
    refreshKey = 0,
    onpatient,
    onerror,
  }: {
    doctor: boolean;
    patient?: Patient | null;
    activeOnly?: boolean;
    refreshKey?: number;
    onpatient?: (id: string) => Promise<void>;
    onerror?: (cause: unknown) => void;
  } = $props();

  let items = $state<Alert[]>([]),
    cursor = $state<AlertPage['next_cursor']>(null);
  let loading = $state(false),
    saving = $state(false),
    error = $state(''),
    notice = $state('');
  let resolveTarget = $state<Alert | null>(null),
    resolution = $state('');
  let routingTarget = $state<Alert | null>(null),
    routingCategory = $state(''),
    routingComment = $state('');
  let alive = false,
    epoch = 0,
    listRequest = 0;

  const statusLabel = (status: string) =>
    ({ new: 'Новый', acknowledged: 'Принят в работу', resolved: 'Обработан' })[status] ?? status;
  const routingLabel = (category: string | null | undefined) =>
    ({
      mandatory: 'Обязательно направить',
      preferred: 'Желательно направить',
      not_indicated: 'Направление не назначено',
    })[category ?? ''] ?? category;

  function fail(cause: unknown) {
    error = cause instanceof Error ? cause.message : 'Не удалось загрузить критические события';
    onerror?.(cause);
  }

  async function load(token: number, more = false) {
    if (more && (loading || !cursor)) return;
    const request = ++listRequest;
    loading = true;
    error = '';
    try {
      const query = {
        ...(more && cursor ? cursor : {}),
        limit: 20,
        ...(doctor && (!patient || activeOnly) ? { status: 'active' } : {}),
      };
      const page = patient
        ? await api('list_patient_alerts', { params: { id: patient.id }, query })
        : await api('list_alerts', { query });
      if (!alive || token !== epoch || request !== listRequest) return;
      items = more ? [...items, ...page.items] : page.items;
      cursor = page.next_cursor;
    } catch (cause) {
      if (alive && token === epoch && request === listRequest) fail(cause);
    } finally {
      if (alive && token === epoch && request === listRequest) loading = false;
    }
  }

  onMount(() => {
    alive = true;
    const refresh = () => {
      const token = ++epoch;
      void load(token);
    };
    window.addEventListener('synapse:alerts-changed', refresh);
    return () => {
      alive = false;
      ++epoch;
      window.removeEventListener('synapse:alerts-changed', refresh);
    };
  });

  $effect(() => {
    const id = patient?.id ?? '',
      refresh = refreshKey,
      token = ++epoch;
    void id;
    void refresh;
    untrack(() => void load(token));
  });

  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!saving) {
        resolveTarget = null;
        routingTarget = null;
      }
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

  async function acknowledge(item: Alert) {
    if (saving) return;
    saving = true;
    error = '';
    try {
      await api('acknowledge_alert', {
        params: { id: item.patient_id, alert_id: item.id },
        body: { version: item.version, comment: '' },
      });
      notice = 'Алерт принят в работу. Прочтение уведомления само по себе этого не делает.';
      await load(epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  async function resolve(event: SubmitEvent) {
    event.preventDefault();
    if (!resolveTarget || saving) return;
    saving = true;
    error = '';
    try {
      await api('resolve_alert', {
        params: { id: resolveTarget.patient_id, alert_id: resolveTarget.id },
        body: { version: resolveTarget.version, comment: resolution },
      });
      resolveTarget = null;
      notice = 'Обработка алерта завершена и сохранена в журнале.';
      await load(epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  async function decideRouting(event: SubmitEvent) {
    event.preventDefault();
    if (!routingTarget || saving) return;
    saving = true;
    error = '';
    try {
      await api('decide_alert_routing', {
        params: { id: routingTarget.patient_id, alert_id: routingTarget.id },
        body: {
          version: routingTarget.version,
          category: routingCategory,
          comment: routingComment,
        },
      });
      routingTarget = null;
      notice = 'Решение о маршрутизации сохранено и зафиксировано в журнале.';
      await load(epoch);
    } catch (cause) {
      fail(cause);
    } finally {
      saving = false;
    }
  }

  async function openPatient(item: Alert) {
    if (!onpatient) return;
    await onpatient(item.patient_id);
    requestAnimationFrame(() =>
      document.getElementById(`alert-${item.id}`)?.scrollIntoView({ block: 'center' }),
    );
  }
</script>

{#if items.length || loading || error}
  <section class="card alert-panel" aria-label="Критические события">
    <div class="section-heading">
      <span class="alert-symbol"><ShieldAlert size={22} /></span>
      <div>
        <h2>{doctor ? 'Критические события' : 'Важное сообщение'}</h2>
        <p class="muted">
          {doctor
            ? 'Чтение уведомления не означает принятие алерта в работу'
            : 'Состояние передачи критических сведений врачу'}
        </p>
      </div>
    </div>
    {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
    {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
    {#if loading && !items.length}<p class="muted" role="status">Загружаем алерты</p>{/if}
    {#if doctor && !patient}
      <div class="alert-table-heading" aria-hidden="true">
        <span>Событие</span><span>Пациент</span><span>Основание и действия</span>
      </div>
    {/if}
    <div class="alert-list" class:alert-table={doctor && !patient}>
      {#each items as item (item.id)}
        <article
          class="critical-alert alert-{item.status}"
          class:wide-alert-row={doctor && !patient}
          id={`alert-${item.id}`}
        >
          <div class="section-heading">
            <div>
              <span class="eyebrow">КРИТИЧЕСКИЙ АЛЕРТ</span>
              <h3>{item.rule_snapshot.title}</h3>
            </div>
            <span class="status">{statusLabel(item.status)}</span>
          </div>
          {#if doctor && !patient}<button
              class="text-link alert-patient-link"
              onclick={() => openPatient(item)}>{item.patient_name}</button
            >{:else if doctor}<p class="alert-patient-name">
              <strong>{item.patient_name}</strong>
            </p>{/if}
          <p class="alert-reason">{item.reason}</p>
          <p class="muted small-text alert-meta">
            Выявлено {date(item.detected_at, true)}. Задержка серверного определения {item.detection_latency_ms}
            мс. Правило {item.rule_code}, версия {item.rule_version}
          </p>
          {#if !doctor}
            <div class="patient-alert-guidance">
              <AlertTriangle size={22} />
              <p>{item.rule_snapshot.patient_guidance}</p>
            </div>
            {#if item.status === 'resolved'}<p class="resolution-note">
                <strong>Решение врача:</strong>
                {item.resolution_comment}
              </p>{/if}
            {#if item.routing_category}<p class="resolution-note">
                <strong>Маршрутизация:</strong>
                {routingLabel(item.routing_category)}: {item.routing_comment}
              </p>{/if}
          {:else}
            {#if item.routing_category}
              <p class="resolution-note alert-routing">
                <strong>Маршрутизация:</strong>
                {routingLabel(item.routing_category)}: {item.routing_comment}
              </p>
            {/if}
            {#if item.status === 'resolved' && item.resolution_comment}<p
                class="resolution-note alert-resolution"
              >
                <strong>Решение врача:</strong>
                {item.resolution_comment}
              </p>{/if}
            {#if item.status !== 'resolved'}
              <div class="clinical-actions">
                {#if item.status === 'new'}<button
                    class="primary"
                    disabled={saving}
                    onclick={() => acknowledge(item)}
                    ><CheckCircle2 size={17} />Принять в работу</button
                  >{/if}
                {#if item.status === 'acknowledged'}<button
                    class="primary"
                    disabled={saving}
                    onclick={() => {
                      resolveTarget = item;
                      resolution = '';
                    }}><CheckCircle2 size={17} />Завершить обработку</button
                  >{/if}
                <button
                  class="secondary"
                  disabled={saving}
                  onclick={() => {
                    routingTarget = item;
                    routingCategory = item.routing_category ?? '';
                    routingComment = item.routing_comment ?? '';
                  }}>Решение о маршрутизации</button
                >
              </div>
            {/if}
          {/if}
        </article>
      {/each}
    </div>
    {#if cursor}<button
        class="secondary full-width"
        disabled={loading}
        onclick={() => load(epoch, true)}>Показать ещё алерты</button
      >{/if}
  </section>
{/if}

{#if resolveTarget && doctor}
  <dialog class="modal card compact-dialog" use:dialog aria-labelledby="resolve-alert-title">
    <form onsubmit={resolve}>
      <span class="eyebrow">{resolveTarget.patient_name}</span>
      <h2 id="resolve-alert-title">Завершить обработку алерта</h2>
      <p>{resolveTarget.reason}</p>
      <label
        >Решение и выполненные действия<textarea
          bind:value={resolution}
          maxlength="4000"
          rows="5"
          required></textarea></label
      >
      <p class="muted">
        Комментарий будет виден пациенту. Назначение или направление выполняются отдельным врачебным
        действием.
      </p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary"
          disabled={saving}
          onclick={() => (resolveTarget = null)}>Назад</button
        ><button class="primary" type="submit" disabled={saving}>Подтвердить обработку</button>
      </div>
    </form>
  </dialog>
{/if}

{#if routingTarget && doctor}
  <dialog class="modal card compact-dialog" use:dialog aria-labelledby="routing-alert-title">
    <form onsubmit={decideRouting}>
      <span class="eyebrow">{routingTarget.patient_name}</span>
      <h2 id="routing-alert-title">Решение о маршрутизации</h2>
      <p>{routingTarget.reason}</p>
      <p class="muted">
        Система показывает основание алерта, но не направляет пациента автоматически. Решение
        принимает врач.
      </p>
      <label
        >Категория
        <select bind:value={routingCategory} required>
          <option value="" disabled>Выберите категорию</option>
          <option value="mandatory">Обязательно направить</option>
          <option value="preferred">Желательно направить</option>
          <option value="not_indicated">Направление не назначено</option>
        </select>
      </label>
      <label
        >Обоснование решения<textarea bind:value={routingComment} maxlength="4000" rows="5" required
        ></textarea></label
      >
      <div class="modal-actions">
        <button
          type="button"
          class="secondary"
          disabled={saving}
          onclick={() => (routingTarget = null)}>Назад</button
        ><button class="primary" type="submit" disabled={saving}>Сохранить решение</button>
      </div>
    </form>
  </dialog>
{/if}
