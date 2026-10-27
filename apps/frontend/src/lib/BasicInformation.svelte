<script lang="ts">
  import type { Patient } from './api';
  let { patient }: { patient: Patient } = $props();
  const number = new Intl.NumberFormat('ru-RU');
  function age(birth: string) {
    const now = new Date(),
      birthday = new Date(`${birth}T12:00:00`);
    let years = now.getFullYear() - birthday.getFullYear();
    if (
      now.getMonth() < birthday.getMonth() ||
      (now.getMonth() === birthday.getMonth() && now.getDate() < birthday.getDate())
    )
      years--;
    const unit = { one: 'год', few: 'года', many: 'лет', other: 'лет', zero: 'лет', two: 'года' }[
      new Intl.PluralRules('ru').select(years)
    ];
    return `${years} ${unit}`;
  }
</script>

<section class="card basic-information">
  <h2>Основная информация</h2>
  <dl>
    <div>
      <dt>Возраст</dt>
      <dd>{age(patient.birth_date)}</dd>
    </div>
    <div>
      <dt>Рост</dt>
      <dd>
        {patient.physical_info.height_cm == null
          ? 'Нет сведений'
          : `${number.format(patient.physical_info.height_cm)} см`}
      </dd>
    </div>
    <div>
      <dt>Вес</dt>
      <dd>
        {patient.physical_info.weight_kg == null
          ? 'Нет сведений'
          : `${number.format(patient.physical_info.weight_kg)} кг`}
      </dd>
    </div>
    <div>
      <dt>Группа крови</dt>
      <dd>{patient.physical_info.blood_group ?? 'Нет сведений'}</dd>
    </div>
  </dl>
  <details class="additional-information">
    <summary>Дополнительные сведения</summary>
    <p>{patient.name}</p>
    <p>Лечащий врач: {patient.doctor_name}</p>
    <p>Аллергии: {patient.allergies ?? 'Нет сведений'}</p>
    {#if patient.medical_summary}<p>{patient.medical_summary}</p>{/if}
  </details>
</section>
