<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { api, ApiError, date, type Patient } from './api';
  import type { components } from './generated/api';
  type Assignment = components['schemas']['CustomQuestionnaireAssignment'];
  type Change = components['schemas']['CustomQuestionnaireChange'];
  let { patient, doctor, refreshKey, onerror, embedded = false }: { patient: Patient; doctor: boolean; refreshKey: number; onerror: (cause: unknown) => void; embedded?: boolean } = $props();
  let records = $state<Assignment[]>([]), nextOffset = $state<number | null>(null);
  let target = $state<Assignment | null>(null), answers = $state<(number | null)[]>([]);
  let loading = $state(false), saving = $state(false), error = $state(''), notice = $state('');
  let auditFor = $state(''), changes = $state<Change[]>([]);
  let epoch = 0, alive = true;
  let complete = $derived(!!target && answers.length === target.spec.questions.length && answers.every((value) => value !== null));
  function fail(cause: unknown) {
    error = cause instanceof Error ? cause.message : 'Не удалось загрузить опросники';
    if (cause instanceof ApiError && cause.status === 401) onerror(cause);
  }
  async function load(more = false) {
    const current = epoch, id = patient.id;
    if (loading || (more && nextOffset === null)) return;
    loading = true; error = '';
    try {
      const page = await api('list_custom_questionnaires', { params: { id }, query: { limit: 30, ...(more ? { offset: nextOffset ?? 0 } : {}) } });
      if (!alive || current !== epoch) return;
      records = more ? [...records, ...page.items.filter((item) => !records.some((old) => old.id === item.id))] : page.items;
      nextOffset = page.next_offset;
    } catch (cause) { if (alive && current === epoch) fail(cause); }
    finally { if (alive && current === epoch) loading = false; }
  }
  function reload() {
    epoch++; loading = false; records = []; nextOffset = null; target = null; error = ''; notice = ''; auditFor = ''; changes = [];
    void load();
  }
  $effect(() => { patient.id; doctor; refreshKey; untrack(reload); });
  onDestroy(() => { alive = false; epoch++; });
  function fill(row: Assignment) { target = row; answers = row.spec.questions.map(() => null); error = ''; notice = ''; }
  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => { event.preventDefault(); if (!saving) target = null; };
    node.addEventListener('cancel', cancel);
    return { destroy() { node.removeEventListener('cancel', cancel); node.close(); previous?.focus(); } };
  }
  async function audit(row: Assignment) {
    const current = epoch;
    try {
      const result = await api('custom_questionnaire_changes', { params: { id: patient.id, assignment_id: row.id } });
      if (alive && current === epoch) { changes = result; auditFor = row.id; }
    } catch (cause) { if (alive && current === epoch) fail(cause); }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!target || saving || !complete) return;
    const current = epoch; saving = true; error = ''; notice = '';
    try {
      await api('submit_custom_questionnaire', { params: { id: patient.id, assignment_id: target.id }, body: { version: target.version, answers: answers as number[] } });
      if (current !== epoch || !alive) return;
      target = null; notice = 'Ответы и результат сохранены.'; await load();
    } catch (cause) { if (current === epoch && alive) fail(cause); }
    finally { if (current === epoch && alive) saving = false; }
  }
  async function cancel(row: Assignment) {
    if (saving) return;
    const current = epoch; saving = true; error = ''; notice = '';
    try {
      await api('cancel_custom_questionnaire', { params: { id: patient.id, assignment_id: row.id }, body: { version: row.version } });
      if (current !== epoch || !alive) return;
      notice = 'Назначение отменено.'; await load();
    } catch (cause) { if (current === epoch && alive) fail(cause); }
    finally { if (current === epoch && alive) saving = false; }
  }
</script>

{#if records.length || loading || error}
  <section class="questionnaires" class:card={!embedded} class:embedded aria-label="Назначенные дополнительные опросники">
    {#if !embedded}<h2>Опросники</h2>{/if}
    {#if error}<p class="feedback error" role="alert">{error}</p><button class="text-link" onclick={reload}>Обновить</button>{/if}
    {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
    {#each records as row (row.id)}
      <article class="questionnaire-record">
        <h3>{row.title}</h3><p class="muted">{row.status === 'assigned' ? 'Ожидает заполнения' : row.status === 'completed' ? 'Завершён' : 'Отменён'}. Назначено {date(row.created_at, true)}. Врач: {row.author_name}</p>
        {#if row.note}<p>{row.note}</p>{/if}
        {#if row.status === 'assigned'}
          {#if doctor}<button class="secondary" disabled={saving} onclick={() => cancel(row)}>Отменить назначение</button>
          {:else}<button class="primary" disabled={saving} onclick={() => fill(row)}>Заполнить</button>{/if}
        {:else if row.status === 'completed'}
          <p><strong>Результат: {row.score} баллов{row.band ? `. ${row.band}` : ''}</strong></p>
          <p class="muted">Результат самооценки рассматривает врач.</p>
          {#if row.draft_id === '92d53473-7d4c-4fc6-9576-c9f32a0b4d09' && (row.answers?.[8] ?? 0) > 0}
            <p class="feedback warning" role="status">Ваш ответ на вопрос о мыслях о смерти или самоповреждении передан лечащему врачу для личной оценки. Если вы прямо сейчас можете причинить себе вред или не уверены, что останетесь в безопасности, обратитесь в местную экстренную службу и попросите близкого человека побыть рядом.</p>
          {/if}
          <details><summary>Сохранённые ответы</summary><ol>{#each row.spec.questions as q, i}<li>{q.text}: {row.answers ? q.options[row.answers[i]]?.label : ''}</li>{/each}</ol></details>
        {/if}
        <details><summary>Источник и условия использования</summary><p>{row.source}</p><p>{row.spec.license}</p><p>{row.spec.limitations}</p></details>
        {#if doctor}<button class="text-link" onclick={() => audit(row)}>Журнал опросника</button>{/if}
        {#if doctor && auditFor === row.id}<div>{#each changes as change}<p class="muted">Версия {change.version}. {change.author_name}, {date(change.created_at, true)}. {change.action === 'assigned' ? 'Назначен' : change.action === 'completed' ? 'Заполнен' : 'Отменён'}</p>{/each}</div>{/if}
      </article>
    {/each}
    {#if nextOffset !== null}<button class="secondary" disabled={loading} onclick={() => load(true)}>Ещё опросники</button>{/if}
    {#if target}
      <dialog class="modal questionnaire-dialog" use:dialog aria-label="Заполнение опросника"><form class="custom-fill" onsubmit={submit}>
        <h3>{target.title}</h3><p>{target.spec.instruction}</p>
        {#each target.spec.questions as q, i (q.id)}<fieldset class="questionnaire-question"><legend>{i + 1}. {q.text}</legend>
          {#each q.options as option, j}<label><input type="radio" name={`custom-${q.id}`} value={j} bind:group={answers[i]} required disabled={saving} /><span>{option.label}</span></label>{/each}
        </fieldset>{/each}
        <div class="custom-actions"><button type="button" class="secondary" disabled={saving} onclick={() => target = null}>Закрыть</button><button class="primary" disabled={saving || !complete}>Отправить ответы</button></div>
      </form></dialog>
    {/if}
  </section>
{/if}

<style>
  .custom-fill { display: grid; gap: 12px; margin: 16px 0; }
  .custom-actions { display: flex; flex-wrap: wrap; gap: 10px; }
</style>
