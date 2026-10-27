<script lang="ts">
  import type { Patient } from './api';
  import { historyFields, comorbidityLabels } from './medicalHistory';
  let { history }: { history: Patient['medical_history'] } = $props();
</script>

<section class="card medical-history" aria-labelledby="medical-history-title">
  <h2 id="medical-history-title">Анамнез и сопутствующие состояния</h2>
  <p class="muted">Сведения, внесённые вашим врачом</p>
  <dl>
    {#each historyFields as [key, label]}
      <div>
        <dt>{label}</dt>
        <dd>{history[key] || 'Нет сведений'}</dd>
      </div>
    {/each}
    <div>
      <dt>Сопутствующие состояния</dt>
      <dd>{comorbidityLabels[history.comorbidities_status]}</dd>
      {#if history.comorbidities_status === 'present'}
        <dd>{history.comorbidities_details}</dd>
      {/if}
    </div>
  </dl>
</section>
