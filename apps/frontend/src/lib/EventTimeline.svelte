<script lang="ts">
import { eventDateLabel, eventKinds, type ObservationEvent } from './api';
let { events, doctor = false }: {
    events: ObservationEvent[];
    doctor?: boolean;
} = $props();
</script>

<div class="timeline event-timeline">
  {#each events as event (event.id)}
    <article class="timeline-item" id={`record-${event.id}`} data-event-id={event.id}>
      <span class="timeline-dot"></span>
      <div class="event-heading">
        <time datetime={event.occurred_at}>{eventDateLabel(event.occurred_at)}</time><span
          class="event-kind">{eventKinds[event.kind] ?? 'Заметка'}</span
        >
      </div>
      <h3>{event.title}</h3>
      <p>{event.body}</p>
      {#if event.source}<p class="event-source"><strong>Источник:</strong> {event.source}</p>{/if}
      <small class="event-recorded"
        >Внёс: {event.author_name}, {eventDateLabel(event.created_at)}</small
      >
      {#if doctor}<small class="event-visibility"
          >{event.patient_visible ? 'Доступно пациенту' : 'Внутренняя запись врача'}</small
        >{/if}
    </article>
  {:else}<p class="muted">Событий пока нет.</p>{/each}
</div>
