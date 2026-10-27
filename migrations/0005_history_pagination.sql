CREATE INDEX diary_patient_cursor_idx ON diary_entries (patient_id, measured_at DESC, created_at DESC, id DESC);
CREATE INDEX event_patient_cursor_idx ON observation_events (patient_id, occurred_at DESC, created_at DESC, id DESC);
