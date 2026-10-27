<script lang="ts">
  import { date, type ProfileChange } from './api';
  import { historyFields, comorbidityLabels } from './medicalHistory';
  let {
    changes,
    hasMore = false,
    busy = false,
    onmore,
  }: {
    changes: ProfileChange[];
    hasMore?: boolean;
    busy?: boolean;
    onmore?: () => Promise<void>;
  } = $props();
  const fields = [
    ['name', 'ФИО'],
    ['birth_date', 'Дата рождения'],
    ['observation_started', 'Начало наблюдения'],
    ['status', 'Статус'],
    ['medical_summary', 'Медицинские сведения'],
    ['allergies', 'Аллергии'],
  ] as const;
  function display(key: string, value: string | number | null | undefined) {
    if (value == null || value === '') return 'Нет сведений';
    if (key === 'status')
      return (
        (
          { active: 'Активный', observation: 'Наблюдение', archived: 'В архиве' } as Record<
            string,
            string
          >
        )[String(value)] ?? String(value)
      );
    if (key === 'birth_date' || key === 'observation_started')
      return new Intl.DateTimeFormat('ru-RU', {
        day: 'numeric',
        month: 'long',
        year: 'numeric',
      }).format(new Date(`${value}T12:00:00`));
    return String(value);
  }
  function historyChanges(change: ProfileChange) {
    const before = change.before_data.medical_history;
    const after = change.after_data.medical_history;
    const rows: { label: string; before: string; after: string }[] = historyFields.map(
      ([key, label]) => ({
        label,
        before: before?.[key] ?? '',
        after: after?.[key] ?? '',
      }),
    );
    rows.push(
      {
        label: 'Сопутствующие состояния',
        before: comorbidityLabels[before?.comorbidities_status ?? 'unknown'],
        after: comorbidityLabels[after?.comorbidities_status ?? 'unknown'],
      },
      {
        label: 'Описание сопутствующих состояний',
        before: before?.comorbidities_details ?? '',
        after: after?.comorbidities_details ?? '',
      },
    );
    for (const [key, label, unit] of [
      ['height_cm', 'Рост', ' см'],
      ['weight_kg', 'Вес', ' кг'],
      ['blood_group', 'Группа крови', ''],
    ] as const) {
      const oldValue = change.before_data.physical_info?.[key];
      const newValue = change.after_data.physical_info?.[key];
      rows.push({
        label,
        before: oldValue == null ? '' : `${oldValue}${unit}`,
        after: newValue == null ? '' : `${newValue}${unit}`,
      });
    }
    return rows.filter((row) => row.before !== row.after);
  }
</script>

<section class="card profile-history">
  <div class="section-heading">
    <div>
      <h2>Изменения карточки</h2>
      <p class="muted">Автор, время и предыдущие значения. Раздел доступен только врачу.</p>
    </div>
  </div>
  {#each changes as change}<details class="profile-change">
      <summary
        ><strong>Версия {change.version_after}</strong><span
          >{date(change.created_at, true)}, {change.author_name}</span
        ></summary
      >
      <dl>
        {#each fields.filter(([key]) => change.before_data[key] !== change.after_data[key]) as [key, label]}<div
            class="change-field"
          >
            <dt>{label}</dt>
            <dd>
              <span class="change-before"
                ><small>Было</small>{display(key, change.before_data[key])}</span
              ><span class="change-after"
                ><small>Стало</small>{display(key, change.after_data[key])}</span
              >
            </dd>
          </div>{/each}
        {#each historyChanges(change) as row}
          <div class="change-field">
            <dt>{row.label}</dt>
            <dd>
              <span class="change-before"><small>Было</small>{row.before || 'Нет сведений'}</span>
              <span class="change-after"><small>Стало</small>{row.after || 'Нет сведений'}</span>
            </dd>
          </div>
        {/each}
      </dl>
    </details>{:else}<p class="muted">
      После редактирования карточки здесь появятся изменения.
    </p>{/each}
  {#if changes.length}<p class="muted">Загружено изменений: {changes.length}</p>{/if}
  {#if hasMore}<button class="secondary" disabled={busy} onclick={onmore}>
      {busy ? 'Загружаем изменения' : 'Загрузить ещё изменения'}
    </button>{/if}
</section>
