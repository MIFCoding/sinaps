<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { components } from './generated/api';
  type Spec = components['schemas']['QuestionnaireDraftSpec'];
  type Question = components['schemas']['QuestionnaireQuestion'];
  type Published = components['schemas']['CustomQuestionnaireVersion'];
  let { draftId, onsaved }: { draftId: string; onsaved: (version: number) => Promise<void> } = $props();
  function question(): Question {
    return { id: crypto.randomUUID(), text: '', options: [{ label: '', weight: 0 }, { label: '', weight: 1 }] };
  }
  let spec = $state<Spec>({ instruction: '', minimum_age: 18, license: '', limitations: '', source_url: null, scoring_url: null, source_sha256: null, attribution: null, license_url: null, checked_on: null, questions: [question()], bands: [] });
  let version = $state(0), loading = $state(true), busy = $state(false), error = $state(''), notice = $state('');
  let published = $state<Published | null>(null), reviewed = $state(false);
  let previewAnswers = $state<Record<string, number>>({});
  let previewScore = $derived(spec.questions.reduce((sum, q) => sum + (q.options[previewAnswers[q.id]]?.weight ?? 0), 0));
  let previewComplete = $derived(spec.questions.every((q) => previewAnswers[q.id] !== undefined && q.options[previewAnswers[q.id]] !== undefined));
  let previewBand = $derived(spec.bands.find((band) => previewScore >= band.min && previewScore <= band.max));
  onMount(() => {
    let active = true;
    Promise.all([api('admin_get_questionnaire_draft', { params: { id: draftId } }), api('admin_questionnaire_catalog')]).then(([result, catalog]) => {
      if (!active) return;
      version = result.version;
      if (result.spec) spec = result.spec;
      published = catalog.find((item) => item.draft_id === draftId) ?? null;
    }).catch((cause) => { if (active) error = cause instanceof Error ? cause.message : 'Не удалось загрузить форму'; })
      .finally(() => { if (active) loading = false; });
    return () => { active = false; };
  });
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (busy || loading) return;
    busy = true; error = ''; notice = '';
    try {
      const result = await api('admin_save_questionnaire_draft', { params: { id: draftId }, body: { expected_version: version, spec } });
      version = result.version;
      notice = 'Форма сохранена как черновик.';
      await onsaved(version);
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось сохранить форму'; }
    finally { busy = false; }
  }
  async function publish() {
    if (busy || !reviewed) return;
    busy = true; error = ''; notice = '';
    try {
      published = await api('admin_publish_questionnaire', { params: { id: draftId }, body: { expected_version: version, reviewed } });
      reviewed = false;
      notice = 'Версия опубликована и доступна врачу для назначения.';
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось опубликовать опросник'; }
    finally { busy = false; }
  }
  async function archive() {
    if (busy || !published) return;
    busy = true; error = ''; notice = '';
    try {
      await api('admin_archive_questionnaire', { params: { id: published.id } });
      published = null;
      notice = 'Версия убрана из списка новых назначений.';
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось архивировать версию'; }
    finally { busy = false; }
  }
</script>

<section class="card questionnaire-editor">
  <h2>Форма опросника</h2>
  <p class="muted">На каждый вопрос даётся один ответ. Итоговый балл равен сумме ответов. Перед подготовкой формы укажите сведения и источник.</p>
  {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
  {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
  {#if loading}<p>Загружаем форму</p>{:else}
    <form onsubmit={save}>
      <label>Инструкция<textarea bind:value={spec.instruction} maxlength="4000" required></textarea></label>
      <label>Минимальный возраст<input type="number" min="0" max="120" bind:value={spec.minimum_age} required /></label>
      <label>Условия использования и лицензия<textarea bind:value={spec.license} maxlength="1000" required></textarea></label>
      <label>Ограничения применения<textarea bind:value={spec.limitations} maxlength="4000" required></textarea></label>
      <details><summary>Проверенные сведения об источнике</summary>
        <label>Источник формы<input type="url" bind:value={spec.source_url} /></label>
        <label>Источник подсчёта<input type="url" bind:value={spec.scoring_url} /></label>
        <label>Ссылка на лицензию<input type="url" bind:value={spec.license_url} /></label>
        <label>Атрибуция<input bind:value={spec.attribution} maxlength="1000" /></label>
        <label>SHA-256 источника<input bind:value={spec.source_sha256} maxlength="64" /></label>
        <label>Дата проверки<input type="date" bind:value={spec.checked_on} /></label>
      </details>
      {#each spec.questions as q, i (q.id)}
        <fieldset><legend>Вопрос {i + 1}</legend>
          <label>Текст вопроса<textarea bind:value={q.text} maxlength="2000" required></textarea></label>
          {#each q.options as option, j}
            <div class="option"><label>Вариант {j + 1}<input bind:value={option.label} maxlength="500" required /></label>
              <label>Баллы<input type="number" min="0" max="100" bind:value={option.weight} required /></label>
              {#if q.options.length > 2}<button class="secondary" type="button" onclick={() => { q.options.splice(j, 1); previewAnswers = {}; }}>Удалить вариант</button>{/if}</div>
          {/each}
          {#if q.options.length < 10}<button class="secondary" type="button" onclick={() => q.options.push({ label: '', weight: 0 })}>Добавить вариант</button>{/if}
          {#if spec.questions.length > 1}<button class="secondary" type="button" onclick={() => { spec.questions.splice(i, 1); previewAnswers = {}; }}>Удалить вопрос</button>{/if}
        </fieldset>
      {/each}
      {#if spec.questions.length < 50}<button class="secondary" type="button" onclick={() => spec.questions.push(question())}>Добавить вопрос</button>{/if}
      <h3>Диапазоны результата</h3>
      <p class="muted">Необязательно. Если указаны, диапазоны должны без пробелов покрывать все возможные суммы.</p>
      {#each spec.bands as band, i}
        <div class="option"><label>От<input type="number" min="0" bind:value={band.min} required /></label><label>До<input type="number" min="0" bind:value={band.max} required /></label><label>Название<input bind:value={band.label} maxlength="500" required /></label><button class="secondary" type="button" onclick={() => spec.bands.splice(i, 1)}>Удалить</button></div>
      {/each}
      {#if spec.bands.length < 30}<button class="secondary" type="button" onclick={() => spec.bands.push({ min: 0, max: 0, label: '' })}>Добавить диапазон</button>{/if}
      <button class="primary" disabled={busy}>{busy ? 'Сохраняем' : 'Сохранить форму'}</button>
    </form>
    <details><summary>Предпросмотр и пример расчёта</summary>
      <p>{spec.instruction}</p>
      {#each spec.questions as q (q.id)}<fieldset><legend>{q.text || 'Вопрос без текста'}</legend>
        {#each q.options as option, j}<label><input type="radio" name={`preview-${q.id}`} checked={previewAnswers[q.id] === j} onchange={() => previewAnswers[q.id] = j} /> {option.label || 'Вариант без текста'}</label>{/each}
      </fieldset>{/each}
      {#if previewComplete}<p>Пример суммы: {previewScore}{previewBand ? `. ${previewBand.label}` : ''}</p>{/if}
      <p class="muted">Пробные ответы не сохраняются и не относятся к пациенту.</p>
    </details>
    <section class="publication">
      <h3>Публикация</h3>
      {#if published}<p>Опубликована версия {published.draft_version}. {published.draft_version !== version ? 'Черновик изменён после публикации.' : ''}</p>
        <button class="secondary" type="button" disabled={busy} onclick={archive}>Архивировать опубликованную версию</button>
      {:else}<p class="muted">Опубликованной версии нет.</p>{/if}
      <label><input type="checkbox" bind:checked={reviewed} /> Я проверил источник, содержание и права использования этой версии.</label>
      <button class="primary" type="button" disabled={busy || !reviewed || published?.draft_version === version} onclick={publish}>Опубликовать сохранённую версию</button>
      <p class="muted">При публикации используются сохранённые данные. Изменения полей формы сначала сохраните.</p>
    </section>
  {/if}
</section>

<style>
  .questionnaire-editor { grid-column: 1 / -1; }
  form, fieldset { display: grid; gap: 10px; }
  label { display: grid; gap: 5px; }
  fieldset { padding: 14px; margin: 8px 0; border: 1px solid #dcebea; border-radius: 12px; }
  .option { display: flex; align-items: end; gap: 10px; flex-wrap: wrap; }
  .option label:first-child { flex: 1 1 220px; }
  details { margin-top: 20px; }
  .publication { display: grid; gap: 10px; margin-top: 24px; }
  details label { display: block; }
</style>
