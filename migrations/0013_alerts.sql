CREATE TABLE alerts (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    source_kind text NOT NULL CHECK (source_kind IN ('adverse_event')),
    source_id uuid NOT NULL,
    source_occurred_at timestamptz NOT NULL,
    source_created_at timestamptz NOT NULL,
    rule_code text NOT NULL,
    rule_version text NOT NULL,
    rule_snapshot jsonb NOT NULL,
    priority text NOT NULL CHECK (priority IN ('critical')),
    reason text NOT NULL,
    status text NOT NULL CHECK (status IN ('new','acknowledged','resolved')),
    version integer NOT NULL CHECK (version >= 1),
    detected_at timestamptz NOT NULL DEFAULT now(),
    detection_latency_ms bigint NOT NULL CHECK (detection_latency_ms >= 0),
    acknowledged_at timestamptz,
    acknowledged_by uuid REFERENCES users(id),
    resolved_at timestamptz,
    resolved_by uuid REFERENCES users(id),
    resolution_comment text NOT NULL CHECK (char_length(resolution_comment) <= 4000),
    CHECK ((status = 'new' AND acknowledged_at IS NULL AND acknowledged_by IS NULL AND resolved_at IS NULL AND resolved_by IS NULL AND resolution_comment = '')
        OR (status = 'acknowledged' AND acknowledged_at IS NOT NULL AND acknowledged_by IS NOT NULL AND resolved_at IS NULL AND resolved_by IS NULL AND resolution_comment = '')
        OR (status = 'resolved' AND acknowledged_at IS NOT NULL AND acknowledged_by IS NOT NULL AND resolved_at IS NOT NULL AND resolved_by IS NOT NULL AND resolution_comment <> '')),
    UNIQUE (source_kind, source_id, rule_code)
);
CREATE INDEX alerts_doctor_queue_idx
    ON alerts(status, priority, detected_at DESC, id DESC);
CREATE INDEX alerts_patient_time_idx
    ON alerts(patient_id, detected_at DESC, id DESC);

CREATE TABLE alert_changes (
    id uuid PRIMARY KEY,
    alert_id uuid NOT NULL REFERENCES alerts(id),
    author_id uuid REFERENCES users(id),
    version integer NOT NULL,
    action text NOT NULL CHECK (action IN ('detected','acknowledged','resolved')),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (alert_id, version)
);

ALTER TABLE notifications DROP CONSTRAINT notifications_kind_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_kind_check
    CHECK (kind IN ('diary_entry','published_event','prescription_published','adverse_event','critical_alert'));
