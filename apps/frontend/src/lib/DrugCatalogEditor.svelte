<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { components } from './generated/api';

  type Spec = components['schemas']['DrugSnapshot'];
  type Published = components['schemas']['DrugCatalogVersion'];
  type DiagnosisChoice = components['schemas']['CatalogDiagnosis'];
  let { draftId, onsaved }: { draftId: string; onsaved: (version: number) => Promise<void> } = $props();
  let version = $state(0), spec = $state<Spec | null>(null);
  let indicationsText = $state(''), effectItems = $state<string[]>([]);
  let diagnoses = $state<DiagnosisChoice[]>([]);
  let diagnosisSearch = $state('');
  let published = $state<Published | null>(null), reviewed = $state(false);
  let loading = $state(true), busy = $state(false), error = $state(''), notice = $state('');
  let filteredDiagnoses = $derived(diagnoses.filter((item) =>
    `${item.code} ${item.title}`.toLocaleLowerCase('ru').includes(diagnosisSearch.trim().toLocaleLowerCase('ru')),
  ).slice(0, 100));

  onMount(() => {
    let active = true;
    Promise.all([
      api('admin_get_drug_draft', { params: { id: draftId } }),
      api('admin_drug_catalog'),
      api('admin_drug_diagnoses'),
    ])
      .then(([draft, catalog, choices]) => {
        if (!active) return;
        version = draft.version;
        const today = new Date().toISOString().slice(0, 10);
        spec = draft.spec ?? {
          code: `drug_${draftId.slice(0, 8)}`,
          brand: '', ingredient: '', dosage_form: '', strength: '', group: '',
          source_title: '', source_date: today, source_url: 'https://',
          source_sha256: '0'.repeat(64), checked_on: today,
          indications: [], diagnosis_codes: [], effects: [], study_note: '', transient_note: '',
        };
        diagnoses = choices;
        indicationsText = spec.indications.join('\n');
        effectItems = spec.effects.map((group) => group.items.join('\n'));
        published = catalog.find((item) => item.draft_id === draftId) ?? null;
      })
      .catch((cause) => { if (active) error = cause instanceof Error ? cause.message : 'Не удалось загрузить справку препарата'; })
      .finally(() => { if (active) loading = false; });
    return () => { active = false; };
  });

  const lines = (value: string) => value.split('\n').map((item) => item.trim()).filter(Boolean);
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!spec || busy || loading) return;
    busy = true; error = ''; notice = '';
    try {
      const result = await api('admin_save_drug_draft', {
        params: { id: draftId },
        body: { expected_version: version, spec: {
          ...spec,
          indications: lines(indicationsText),
          diagnosis_codes: spec.diagnosis_codes,
          effects: spec.effects.map((group, index) => ({ ...group, items: lines(effectItems[index] ?? '') })),
        } },
      });
      version = result.version;
      spec = result.spec;
      if (spec) {
        indicationsText = spec.indications.join('\n');
        effectItems = spec.effects.map((group) => group.items.join('\n'));
      }
      notice = 'Справка сохранена как черновик.';
      await onsaved(version);
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось сохранить справку'; }
    finally { busy = false; }
  }

  async function publish() {
    if (busy || !reviewed) return;
    busy = true; error = ''; notice = '';
    try {
      published = await api('admin_publish_drug', { params: { id: draftId }, body: { expected_version: version, reviewed } });
      reviewed = false;
      notice = 'Справка опубликована для врача и новых назначений.';
    } catch (cause) { error = cause instanceof Error ? cause.message : 'Не удалось опубликовать справку'; }
    finally { busy = false; }
  }
</script>

<section class="card drug-editor">
  <h2>Карточка препарата</h2>
  <p class="muted">Опубликованная версия используется в новых назначениях. Уже созданные назначения сохраняют свою справку.</p>
  {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
  {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
  {#if loading}<p role="status">Загружаем справку</p>{:else if spec}
    <form onsubmit={save}>
      <label>Код препарата<input bind:value={spec.code} required minlength="2" maxlength="64" pattern="[a-z0-9_-]+" /></label>
      <div class="form-row"><label>Торговое наименование<input bind:value={spec.brand} required maxlength="200" /></label>
        <label>Действующее вещество<input bind:value={spec.ingredient} required maxlength="200" /></label></div>
      <div class="form-row"><label>Лекарственная форма<input bind:value={spec.dosage_form} maxlength="300" placeholder="Например, таблетки" /></label>
        <label>Сила дозировки<input bind:value={spec.strength} maxlength="200" placeholder="Например, 20 мг" /></label></div>
      <label>Фармакологическая группа<input bind:value={spec.group} required maxlength="300" /></label>
      <label>Источник<textarea bind:value={spec.source_title} required maxlength="1000" rows="2"></textarea></label>
      <div class="form-row"><label>Дата документа<input type="date" bind:value={spec.source_date} required /></label>
        <label>Дата проверки<input type="date" bind:value={spec.checked_on} required /></label></div>
      <label>Ссылка на источник<input type="url" bind:value={spec.source_url} required /></label>
      <label>SHA-256 источника<input bind:value={spec.source_sha256} required maxlength="64" /></label>
      <label>Показания по источнику<textarea bind:value={indicationsText} rows="4" required placeholder="Один пункт на строку"></textarea></label>
      <fieldset><legend>Диагнозы, при которых врач может выбрать препарат</legend>
        {#if diagnoses.length === 0}<p class="muted">Сначала добавьте и опубликуйте диагнозы в каталоге.</p>{/if}
        <label>Найти диагноз<input bind:value={diagnosisSearch} placeholder="Код или название" /></label>
        {#each filteredDiagnoses as diagnosis (diagnosis.code)}
          <label class="diagnosis-option"><input type="checkbox" checked={spec.diagnosis_codes.includes(diagnosis.code)} onchange={(event) => {
            spec!.diagnosis_codes = event.currentTarget.checked
              ? [...spec!.diagnosis_codes, diagnosis.code]
              : spec!.diagnosis_codes.filter((code) => code !== diagnosis.code);
          }} />{diagnosis.code}: {diagnosis.title}</label>
        {/each}
        {#if filteredDiagnoses.length === 100}<p class="muted">Показаны первые 100 совпадений. Уточните поиск.</p>{/if}
        <p class="muted">Выбрано диагнозов: {spec.diagnosis_codes.length}</p>
      </fieldset>
      <h3>Нежелательные реакции</h3>
      {#each spec.effects as group, index (index)}
        <fieldset><legend>Группа реакций {index + 1}</legend>
          <label>Частота<input bind:value={group.frequency} required maxlength="300" /></label>
          <label>Реакции<textarea bind:value={effectItems[index]} rows="5" required placeholder="Одна реакция на строку"></textarea></label>
          {#if spec.effects.length > 1}<button class="secondary" type="button" onclick={() => { spec!.effects.splice(index, 1); effectItems.splice(index, 1); }}>Удалить группу</button>{/if}
        </fieldset>
      {/each}
      <button class="secondary" type="button" disabled={spec.effects.length >= 20} onclick={() => { spec!.effects.push({ frequency: '', items: [] }); effectItems.push(''); }}>Добавить группу частоты</button>
      <label>Сведения об исследованиях<textarea bind:value={spec.study_note} rows="3" required maxlength="4000"></textarea></label>
      <label>Временные реакции и пояснение<textarea bind:value={spec.transient_note} rows="3" required maxlength="4000"></textarea></label>
      <button class="primary" disabled={busy}>{busy ? 'Сохраняем' : 'Сохранить справку'}</button>
    </form>
    <section class="publication">
      <h3>Публикация</h3>
      {#if published}<p>Активна версия {published.draft_version}. Новая версия заменит её для новых назначений.</p>
      {:else}<p class="muted">Опубликованной версии нет.</p>{/if}
      <label class="review"><input type="checkbox" bind:checked={reviewed} /> Я проверил актуальность источника, сведения и нежелательные реакции.</label>
      <button class="primary" type="button" disabled={busy || !reviewed} onclick={publish}>Опубликовать сохранённую версию</button>
      <p class="muted">Перед публикацией сохраните изменения. История назначений и реакций остаётся привязана к сохранённым снимкам.</p>
    </section>
  {:else}<p class="feedback error" role="alert">В черновике нет структурированной справки.</p>{/if}
</section>

<style>
  .drug-editor { grid-column: 1 / -1; }
  form, fieldset, .publication { display: grid; gap: 12px; }
  fieldset { border: 1px solid #dcebea; border-radius: 12px; padding: 14px; }
  label { display: grid; gap: 6px; }
  .review { display: flex; gap: 8px; align-items: flex-start; }
  .diagnosis-option { display: flex; grid-template-columns: 18px 1fr; align-items: start; gap: 8px; }
  .diagnosis-option input { width: auto; margin-top: 4px; }
  .review input { width: auto; margin-top: 4px; }
  .publication { margin-top: 24px; }
  @media (max-width: 600px) { .form-row { display: grid; } }
</style>
