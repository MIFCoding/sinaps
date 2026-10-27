<script lang="ts">
  import { onMount } from 'svelte';
  import Brand from './Brand.svelte';
  import QuestionnaireEditor from './QuestionnaireEditor.svelte';
  import ClinicalEntryEditor from './ClinicalEntryEditor.svelte';
  import DrugCatalogEditor from './DrugCatalogEditor.svelte';
  import { api } from './api';
  import type { components } from './generated/api';

  let { onlogout }: { onlogout: () => Promise<void> } = $props();
  type Draft = components['schemas']['CatalogDraft'];
  const kinds: Record<string, string> = {
    questionnaire: 'Опросник',
    disease: 'Заболевание',
    finding: 'Симптом или критерий',
    drug: 'Препарат',
    adverse_effect: 'Нежелательная реакция',
  };
  let drafts = $state<Draft[]>([]);
  let selected = $state<Draft | null>(null);
  let kind = $state('questionnaire'), title = $state(''), source = $state(''), details = $state('');
  let error = $state(''), notice = $state(''), busy = $state(false);
  const gad7DraftId = '3ab28556-f1b4-5d19-ae35-4bd7777260ac';
  const aviandrDraftId = '8ea5348e-ef47-5be1-bc69-26a35a35a228';
  let hasGad7Draft = $derived(drafts.some((draft) => draft.id === gad7DraftId));
  let hasAviandrDraft = $derived(drafts.some((draft) => draft.id === aviandrDraftId));

  async function load() {
    try { drafts = await api('admin_list_drafts'); }
    catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось загрузить черновики'; }
  }
  onMount(() => { void load(); });
  async function importGad7() {
    if (busy || hasGad7Draft) return;
    busy = true; error = ''; notice = '';
    try {
      const draft = await api('admin_import_gad7');
      await load();
      edit(draft);
      notice = 'Проверенная форма GAD‑7 добавлена в каталог как черновик. Проверьте источник и опубликуйте её.';
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось добавить GAD‑7'; }
    finally { busy = false; }
  }
  async function importAviandr() {
    if (busy || hasAviandrDraft) return;
    busy = true; error = ''; notice = '';
    try {
      const draft = await api('admin_import_aviandr');
      await load();
      edit(draft);
      notice = 'Проверенная справка Авиандра добавлена в каталог как черновик. Проверьте сведения и опубликуйте её.';
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось добавить справку Авиандра'; }
    finally { busy = false; }
  }
  function edit(draft: Draft | null) {
    selected = draft;
    kind = draft?.kind ?? 'questionnaire';
    title = draft?.title ?? '';
    source = draft?.source ?? '';
    details = draft?.details ?? '';
    error = '';
    notice = '';
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const draft = { kind, title, source, details };
      let saved: Draft;
      if (selected) {
        saved = await api('admin_update_draft', { params: { id: selected.id }, body: { ...draft, expected_version: selected.version } });
      } else {
        saved = await api('admin_create_draft', { body: draft });
      }
      await load();
      edit(saved);
      notice = 'Черновик сохранён. Заполните структурированные поля ниже.';
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Не удалось сохранить черновик';
    } finally { busy = false; }
  }
</script>

<main class="admin-page">
  <header class="admin-header"><Brand /><button class="secondary" onclick={onlogout}>Выйти</button></header>
  <h1>Кабинет администратора</h1>
  <p>Черновики справочников</p>
  <p class="muted">Черновики становятся доступны врачу после проверки и публикации.</p>
  {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
  {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
  <div class="admin-grid">
    <section class="card">
      <h2>Подготовленные формы</h2>
      <p class="muted">Проверенная форма GAD‑7 может быть добавлена в каталог и опубликована после проверки.</p>
      {#if !hasGad7Draft}<button class="secondary" type="button" disabled={busy} onclick={importGad7}>Добавить GAD‑7 в каталог</button>
      {:else}<p class="muted">GAD‑7 находится в черновиках. Откройте его в списке, чтобы проверить и опубликовать.</p>{/if}
      {#if !hasAviandrDraft}<button class="secondary" type="button" disabled={busy} onclick={importAviandr}>Добавить Авиандр в каталог</button>
      {:else}<p class="muted">Справка Авиандра находится в черновиках. Откройте её, чтобы проверить и опубликовать.</p>{/if}
    </section>
    <section class="card">
      <h2>{selected ? 'Редактировать черновик' : 'Новый черновик'}</h2>
      <form class="admin-form" onsubmit={save}>
        <label for="admin-kind">Раздел</label>
        <select id="admin-kind" bind:value={kind}>{#each Object.entries(kinds) as [value, label]}<option {value}>{label}</option>{/each}</select>
        <label for="admin-title">Название</label>
        <input id="admin-title" bind:value={title} maxlength="200" required />
        <label for="admin-source">Источник</label>
        <textarea id="admin-source" bind:value={source} maxlength="2000" rows="3" placeholder="Ссылка, название документа, версия и условия использования"></textarea>
        <label for="admin-details">Сведения</label>
        <textarea id="admin-details" bind:value={details} maxlength="20000" rows="8" placeholder="Описание и данные для подготовки справочника"></textarea>
        <div class="admin-actions"><button class="primary" disabled={busy}>{busy ? 'Сохраняем' : 'Сохранить черновик'}</button>
          {#if selected}<button class="secondary" type="button" onclick={() => edit(null)}>Новый черновик</button>{/if}</div>
      </form>
    </section>
    {#if selected?.kind === 'questionnaire'}
      {#key `${selected.id}:${selected.version}`}
        <QuestionnaireEditor draftId={selected.id} onsaved={async (version) => {
          if (selected) selected = { ...selected, version };
          await load();
        }} />
      {/key}
    {/if}
    {#if selected?.kind === 'disease' || selected?.kind === 'finding'}
      {#key `${selected.id}:${selected.version}`}
        <ClinicalEntryEditor draftId={selected.id} title={selected.title} onsaved={async (version) => {
          if (selected) selected = { ...selected, version };
          await load();
        }} />
      {/key}
    {/if}
    {#if selected?.kind === 'drug'}
      {#key `${selected.id}:${selected.version}`}
        <DrugCatalogEditor draftId={selected.id} onsaved={async (version) => {
          if (selected) selected = { ...selected, version };
          await load();
        }} />
      {/key}
    {/if}
    <section class="card"><h2>Сохранённые черновики</h2>
      {#if drafts.length === 0}<p class="muted">Черновиков пока нет.</p>{/if}
      {#each drafts as draft (draft.id)}<button class="admin-item" onclick={() => edit(draft)}><strong>{draft.title}</strong><span>{kinds[draft.kind] ?? draft.kind}, версия {draft.version}</span></button>{/each}
      {#if drafts.length === 500}<p class="muted">Показаны 500 последних записей.</p>{/if}
    </section>
  </div>
</main>

<style>
  .admin-page { max-width: 1100px; margin: 0 auto; padding: 24px; }
  .admin-header { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 32px; }
  .admin-grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 20px; align-items: start; margin-top: 24px; }
  .admin-form { display: grid; gap: 10px; }
  .admin-form label { margin-top: 8px; }
  .admin-actions { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 12px; }
  .admin-item { display: grid; gap: 5px; width: 100%; padding: 14px; text-align: left; border: 1px solid #dcebea; border-radius: 12px; background: white; margin-top: 10px; cursor: pointer; }
  .admin-item span { font-size: 0.85rem; color: #6b8794; }
  @media (max-width: 700px) { .admin-page { padding: 16px; } .admin-grid { grid-template-columns: 1fr; } }
</style>
