<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { components } from './generated/api';

  type Spec = components['schemas']['ClinicalEntryDraftSpec'];
  type Entry = components['schemas']['ClinicalEntryVersion'];
  let {
    draftId,
    title,
    onsaved,
  }: { draftId: string; title: string; onsaved: (version: number) => Promise<void> } = $props();
  let version = $state(0);
  let kind = $state<'disease' | 'finding'>('disease');
  let spec = $state<Spec>({ code: '', source_code: '', context: '', source_page: 1 });
  let published = $state<Entry | null>(null);
  let reviewed = $state(false);
  let loading = $state(true), busy = $state(false), error = $state(''), notice = $state('');

  onMount(() => {
    let active = true;
    Promise.all([
      api('admin_get_clinical_entry_draft', { params: { id: draftId } }),
      api('admin_clinical_catalog'),
    ]).then(([draft, entries]) => {
      if (!active) return;
      version = draft.version;
      kind = draft.kind === 'finding' ? 'finding' : 'disease';
      if (draft.spec) spec = draft.spec;
      published = entries.find((entry) => entry.draft_id === draftId) ?? null;
    }).catch((cause) => {
      if (active) error = cause instanceof Error ? cause.message : 'Не удалось загрузить клиническую запись';
    }).finally(() => { if (active) loading = false; });
    return () => { active = false; };
  });

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (busy || loading) return;
    busy = true; error = ''; notice = '';
    try {
      const result = await api('admin_save_clinical_entry_draft', {
        params: { id: draftId }, body: { expected_version: version, spec },
      });
      version = result.version;
      spec = result.spec ?? spec;
      notice = 'Сведения сохранены в черновике.';
      await onsaved(version);
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось сохранить сведения'; }
    finally { busy = false; }
  }

  async function publish() {
    if (busy || !reviewed) return;
    busy = true; error = ''; notice = '';
    try {
      published = await api('admin_publish_clinical_entry', {
        params: { id: draftId }, body: { expected_version: version, reviewed },
      });
      reviewed = false;
      notice = 'Запись опубликована в каталоге врача.';
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось опубликовать запись'; }
    finally { busy = false; }
  }
</script>

<section class="card clinical-entry-editor">
  <h2>{kind === 'disease' ? 'Запись заболевания' : 'Запись проявления'}</h2>
  <p class="muted">После публикации врач сможет выбрать эту запись в карточке пациента. Ранее сохранённые диагнозы не меняются.</p>
  {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
  {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
  {#if loading}<p role="status">Загружаем сведения</p>{:else}
    <form onsubmit={save}>
      <p><strong>{title}</strong></p>
      {#if kind === 'disease'}
        <label>Код классификации<input bind:value={spec.code} maxlength="40" required placeholder="Например, F41.1" /></label>
        <p class="muted">В этом разделе используются коды МКБ-10. Укажите источник в общих сведениях черновика.</p>
      {:else}
        <label>Код заболевания, к которому относится проявление<input bind:value={spec.source_code} maxlength="40" required placeholder="Например, F41.1" /></label>
        <label>Описание и контекст пункта<textarea bind:value={spec.context} maxlength="4000" rows="4" required></textarea></label>
        <label>Страница источника<input type="number" bind:value={spec.source_page} min="1" max="20000" required /></label>
      {/if}
      <button class="primary" disabled={busy}>{busy ? 'Сохраняем' : 'Сохранить сведения'}</button>
    </form>
    <section class="publication">
      <h3>Публикация</h3>
      {#if published}<p>Опубликована версия {published.draft_version}; новая версия заменит её в новых выборах.</p>
      {:else}<p class="muted">Опубликованной версии нет.</p>{/if}
      <label class="review"><input type="checkbox" bind:checked={reviewed} /> Я проверил содержание, связь с заболеванием, источник и права использования.</label>
      <button class="primary" type="button" disabled={busy || !reviewed} onclick={publish}>Опубликовать сохранённую запись</button>
      <p class="muted">Сначала сохраните форму. Опубликованные сведения не меняют уже сохранённые диагнозы.</p>
    </section>
  {/if}
</section>

<style>
  .clinical-entry-editor { grid-column: 1 / -1; }
  form, .publication { display: grid; gap: 12px; }
  label { display: grid; gap: 6px; }
  .review { display: flex; align-items: flex-start; gap: 8px; }
  .review input { width: auto; margin-top: 4px; }
  .publication { margin-top: 24px; }
</style>
