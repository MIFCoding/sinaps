<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { api, ApiError, type Patient } from './api';
  import type { components } from './generated/api';

  type Recommendation = components['schemas']['ClinicalRecommendation'];
  let {
    patient,
    doctor,
    refreshKey,
    onerror,
  }: {
    patient: Patient;
    doctor: boolean;
    refreshKey: number;
    onerror: (cause: unknown) => void;
  } = $props();

  let rows = $state<Recommendation[]>([]);
  let busy = $state(false);
  let error = $state('');
  let comment = $state('');
  let epoch = 0;
  let alive = true;
  const routeLabel = { mandatory: 'Требуется обязательная оценка', preferred: 'Рекомендуется обсудить' };
  const statusLabel = {
    proposed: 'Ожидает решения врача',
    accepted: 'Принято врачом',
    declined: 'Отклонено врачом',
    published: 'Опубликовано пациенту',
    withdrawn: 'Снято после изменения диагноза',
  };
  function evidenceLabel(row: Recommendation) {
    const evidence = row.evidence as Record<string, unknown>;
    if (typeof evidence.diagnosis_code === 'string') {
      return `Диагноз ${evidence.diagnosis_code}, внесён врачом`;
    }
    if (typeof evidence.question_index === 'number') {
      return `Ответ на пункт ${evidence.question_index + 1} опросника PHQ‑9`;
    }
    return 'Данные исходного правила сохранены';
  }

  async function load() {
    const token = ++epoch;
    error = '';
    try {
      const result = await api('list_clinical_recommendations', { params: { id: patient.id } });
      if (alive && token === epoch) rows = result;
    } catch (cause) {
      if (!alive || token !== epoch) return;
      error = cause instanceof Error ? cause.message : 'Не удалось загрузить рекомендации';
      if (cause instanceof ApiError && cause.status === 401) onerror(cause);
    }
  }

  $effect(() => {
    patient.id;
    doctor;
    refreshKey;
    untrack(() => void load());
  });

  onDestroy(() => {
    alive = false;
    epoch++;
  });

  async function decide(row: Recommendation, decision: 'accept' | 'decline') {
    if (busy) return;
    const token = epoch;
    busy = true;
    error = '';
    try {
      await api('decide_clinical_recommendation', {
        params: { id: patient.id, recommendation_id: row.id },
        body: { expected_version: row.version, decision, comment },
      });
      if (alive && token === epoch) {
        comment = '';
        await load();
      }
    } catch (cause) {
      if (alive && token === epoch) {
        error = cause instanceof Error ? cause.message : 'Не удалось сохранить решение';
        if (cause instanceof ApiError && cause.status === 401) onerror(cause);
      }
    } finally {
      if (alive && token === epoch) busy = false;
    }
  }

  async function publish(row: Recommendation) {
    if (busy) return;
    const token = epoch;
    busy = true;
    error = '';
    try {
      await api('publish_clinical_recommendation', {
        params: { id: patient.id, recommendation_id: row.id },
        body: { expected_version: row.version },
      });
      if (alive && token === epoch) await load();
    } catch (cause) {
      if (alive && token === epoch) {
        error = cause instanceof Error ? cause.message : 'Не удалось опубликовать рекомендацию';
        if (cause instanceof ApiError && cause.status === 401) onerror(cause);
      }
    } finally {
      if (alive && token === epoch) busy = false;
    }
  }
</script>

<section class="card clinical-recommendations" aria-label="Клинические рекомендации">
  <div class="section-heading">
    <div>
      <h2>{doctor ? 'Рекомендации и маршрутизация' : 'План наблюдения'}</h2>
      <p class="muted">{doctor ? 'Обоснованные предложения остаются на проверке врача' : 'Рекомендации, опубликованные лечащим врачом'}</p>
    </div>
  </div>
  {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
  {#each rows as row (row.id)}
    <article class="clinical-recommendation" data-recommendation-id={row.id}>
      <div class="clinical-recommendation-heading">
        <strong>{row.title}</strong>
        <span class="status" class:observation={row.status === 'proposed'}>{doctor ? statusLabel[row.status as keyof typeof statusLabel] : routeLabel[row.suggested_route as keyof typeof routeLabel]}</span>
      </div>
      {#if doctor}<p class="muted">Маршрут: {routeLabel[row.suggested_route as keyof typeof routeLabel]}</p>{/if}
      <p>{row.rationale}</p>
      {#if row.decision_comment}<p class="clinical-recommendation-comment">Решение врача: {row.decision_comment}</p>{/if}
      <p class="muted small-text">Основание: {evidenceLabel(row)}</p>
      <a class="text-link" href={row.source_url} target="_blank" rel="noreferrer">Источник правила</a>
      {#if doctor && row.status === 'proposed'}
        <label for={`recommendation-comment-${row.id}`}>Комментарий к решению</label>
        <textarea id={`recommendation-comment-${row.id}`} rows="2" maxlength="4000" bind:value={comment}></textarea>
        <div class="clinical-recommendation-actions">
          <button class="secondary" disabled={busy} onclick={() => decide(row, 'decline')}>Отклонить</button>
          <button class="primary" disabled={busy} onclick={() => decide(row, 'accept')}>Принять предложение</button>
        </div>
      {:else if doctor && row.status === 'accepted'}
        <button class="primary" disabled={busy} onclick={() => publish(row)}>Опубликовать пациенту</button>
      {/if}
    </article>
  {:else}
    <p class="muted">{doctor ? 'Рекомендаций для текущих данных нет.' : 'План наблюдения ещё не опубликован.'}</p>
  {/each}
  {#if doctor}<p class="muted small-text recommendation-disclaimer">Система не ставит диагноз и не назначает лечение. Решение и публикация остаются за врачом.</p>{/if}
</section>
