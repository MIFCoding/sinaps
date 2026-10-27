<script lang="ts">
  import type { DiaryEntry } from './api';
  import { date } from './api';

  let {
    entries,
    metric,
    label,
    color,
    unit,
    max = 10,
  }: {
    entries: DiaryEntry[];
    metric: 'mood' | 'anxiety' | 'sleep_hours';
    label: string;
    color: string;
    unit: string;
    max?: number;
  } = $props();

  const width = 560;
  const height = 128;
  const left = 34;
  const right = 548;
  const top = 12;
  const bottom = 98;
  let ordered = $derived(
    [...entries].sort((a, b) => Date.parse(a.measured_at) - Date.parse(b.measured_at)),
  );
  let upper = $derived(
    metric === 'sleep_hours' ? Math.max(max, ...ordered.map((entry) => entry[metric])) : max,
  );
  let firstMeasured = $derived(ordered.length ? Date.parse(ordered[0].measured_at) : 0);
  let lastMeasured = $derived(ordered.length ? Date.parse(ordered.at(-1)!.measured_at) : 0);
  const day = 86400000;
  let start = $derived(
    ordered.length ? firstMeasured - Math.max(0, day - (lastMeasured - firstMeasured)) / 2 : 0,
  );
  let end = $derived(
    ordered.length
      ? Math.max(lastMeasured + Math.max(0, day - (lastMeasured - firstMeasured)) / 2, start + 1)
      : 0,
  );
  let span = $derived(Math.max(1, end - start));
  const x = (value: number) => left + ((value - start) / span) * (right - left);
  const y = (value: number) => bottom - (value / upper) * (bottom - top);
  let points = $derived(
    ordered.map((entry) => ({
      x: x(Date.parse(entry.measured_at)),
      y: y(entry[metric]),
      entry,
    })),
  );
  let dateTicks = $derived.by(() => {
    if (!ordered.length) return [];
    const candidates = [start, start + span / 2, end];
    const unique: number[] = [];
    for (const value of candidates) {
      if (!unique.length || x(value) - x(unique.at(-1)!) > 58) unique.push(value);
    }
    if (unique.length === 1 && end > start) unique.push(end);
    return unique.map((value, index) => ({
      x: x(value),
      label: date(new Date(value).toISOString()),
      anchor: index === 0 ? 'start' : index === unique.length - 1 ? 'end' : 'middle',
    }));
  });
  let latest = $derived(ordered.at(-1));
</script>

<article class="metric-chart">
  <div class="chart-heading">
    <span class="metric-name"><span class="dot" style:background={color}></span>{label}</span>
    <span class="chart-number">
      {#if latest}{latest[metric]} <small>{unit}</small>{:else}<span class="muted">Нет данных</span>{/if}
    </span>
  </div>
  {#if points.length}
    <div class="plot">
      <svg
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={`${label}: ${ordered.length} измерений, шкала от 0 до ${upper}`}
      >
        {#each [upper, upper / 2, 0] as value}
          <line
            x1={left}
            x2={right}
            y1={y(value)}
            y2={y(value)}
            stroke="#e6eff1"
            stroke-dasharray={value === 0 ? undefined : '3 5'}
          />
          <text x="25" y={y(value) + 3} text-anchor="end" class="axis-label">{value}</text>
        {/each}
        {#if points.length > 1}
          <polyline
            points={points.map((point) => `${point.x},${point.y}`).join(' ')}
            fill="none"
            stroke={color}
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        {/if}
        {#each points as point}
          <circle cx={point.x} cy={point.y} r="3.5" fill="white" stroke={color} stroke-width="2">
            <title>{date(point.entry.measured_at, true)}: {point.entry[metric]} {unit}</title>
          </circle>
        {/each}
        {#each dateTicks as tick}
          <text x={tick.x} y="120" text-anchor={tick.anchor} class="axis-label">{tick.label}</text>
        {/each}
      </svg>
    </div>
  {:else}<div class="empty-chart">За этот период измерений пока нет</div>{/if}
</article>
