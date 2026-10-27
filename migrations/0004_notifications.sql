CREATE TABLE notifications (
    id uuid PRIMARY KEY,
    recipient_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('diary_entry', 'published_event')),
    related_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    read_at timestamptz,
    UNIQUE (recipient_id, kind, related_id)
);

CREATE INDEX notifications_recipient_time_idx ON notifications (recipient_id, created_at DESC, id DESC);
CREATE INDEX notifications_unread_idx ON notifications (recipient_id) WHERE read_at IS NULL;
