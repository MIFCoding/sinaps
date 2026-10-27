<script lang="ts">
  import type { components } from './generated/api';
  type Assignment = components['schemas']['QuestionnaireAssignment'];
  type Result = Assignment & { score: number; completed_at: string };
  let { records }: { records: Assignment[] } = $props();
  let results = $derived(
    records
      .filter(
        (row): row is Result =>
          row.status === 'completed' && row.score !== null && row.completed_at !== null,
      )
      .sort((a, b) => Date.parse(a.completed_at) - Date.parse(b.completed_at)),
  );
  let first = $derived(results.length ? Date.parse(results[0].completed_at) : 0);
  let span = $derived(
    results.length ? Math.max(1, Date.parse(results.at(-1)!.completed_at) - first) : 1,
  );
  const x = (row: Result) =>
    results.length === 1 ? 260 : 36 + ((Date.parse(row.completed_at) - first) / span) * 464;
  const y = (score: number) => 146 - (score / 21) * 124;
  let points = $derived(results.map((row) => `${x(row)},${y(row.score)}`).join(' '));
</script>

<div class="questionnaire-trend">
  <h3>Динамика GAD‑7</h3>
  <p class="muted">
    Баллы 0–21 по загруженным завершённым опросникам. Отдельно от дневниковых самооценок.
  </p>
  {#if results.length}
    <svg viewBox="0 0 524 176" role="img" aria-label="Динамика баллов GAD-7">
      {#each [0, 7, 14, 21] as score}<line
          x1="36"
          x2="500"
          y1={y(score)}
          y2={y(score)}
          stroke="var(--line)"
          stroke-dasharray="3 5"
        /><text x="26" y={y(score) + 4} text-anchor="end">{score}</text>{/each}
      {#if results.length > 1}<polyline
          {points}
          fill="none"
          stroke="var(--anxiety)"
          stroke-width="2.5"
        />{/if}
      {#each results as row}<circle
          cx={x(row)}
          cy={y(row.score)}
          r="4"
          fill="white"
          stroke="var(--anxiety)"
          stroke-width="2.5"
          ><title>{new Date(row.completed_at).toLocaleString('ru-RU')}, {row.score} из 21</title
          ></circle
        >{/each}
      <text x="36" y="170">{new Date(results[0].completed_at).toLocaleDateString('ru-RU')}</text>
      {#if results.length > 1}<text x="500" y="170" text-anchor="end"
          >{new Date(results.at(-1)!.completed_at).toLocaleDateString('ru-RU')}</text
        >{/if}
    </svg>
    <p class="muted small-text">Точные даты, баллы и ответы доступны в записях ниже.</p>
  {:else}<p class="muted">После заполнения опросника здесь появится результат.</p>{/if}
</div>
