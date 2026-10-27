ALTER TABLE observation_events
    ADD COLUMN kind text NOT NULL DEFAULT 'note'
        CHECK (kind IN ('initial_assessment', 'consultation', 'plan_change', 'note', 'other')),
    ADD COLUMN source text CHECK (char_length(source) <= 500),
    ADD COLUMN occurred_at_provided boolean NOT NULL DEFAULT false;

CREATE INDEX event_patient_chronology_idx
    ON observation_events (patient_id, occurred_at DESC, created_at DESC, id DESC);
