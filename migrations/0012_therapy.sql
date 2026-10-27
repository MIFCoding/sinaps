CREATE TABLE prescriptions (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    author_id uuid NOT NULL REFERENCES users(id),
    updated_by uuid NOT NULL REFERENCES users(id),
    drug_code text NOT NULL CHECK (drug_code = 'aviandr'),
    drug_snapshot jsonb NOT NULL,
    dosage text NOT NULL CHECK (char_length(dosage) BETWEEN 1 AND 200),
    intake_times jsonb NOT NULL,
    starts_on date NOT NULL,
    ends_on date,
    instructions text NOT NULL CHECK (char_length(instructions) <= 2000),
    status text NOT NULL CHECK (status IN ('draft','active','cancelled','completed')),
    version integer NOT NULL CHECK (version >= 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    published_at timestamptz,
    cancelled_at timestamptz,
    CHECK (ends_on IS NULL OR ends_on >= starts_on),
    CHECK ((status = 'draft' AND published_at IS NULL AND cancelled_at IS NULL)
        OR (status = 'active' AND published_at IS NOT NULL AND cancelled_at IS NULL)
        OR (status = 'completed' AND published_at IS NOT NULL AND cancelled_at IS NULL)
        OR (status = 'cancelled' AND cancelled_at IS NOT NULL))
);

CREATE UNIQUE INDEX prescription_one_current_drug_idx
    ON prescriptions(patient_id, drug_code)
    WHERE status IN ('draft','active');
CREATE INDEX prescription_patient_time_idx
    ON prescriptions(patient_id, created_at DESC, id DESC);

CREATE TABLE prescription_changes (
    id uuid PRIMARY KEY,
    prescription_id uuid NOT NULL REFERENCES prescriptions(id),
    author_id uuid NOT NULL REFERENCES users(id),
    version integer NOT NULL,
    action text NOT NULL CHECK (action IN ('created','updated','published','cancelled','completed')),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (prescription_id, version)
);

CREATE TABLE adherence_marks (
    id uuid PRIMARY KEY,
    prescription_id uuid NOT NULL REFERENCES prescriptions(id),
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    scheduled_on date NOT NULL,
    scheduled_time time NOT NULL,
    status text NOT NULL CHECK (status IN ('taken','skipped')),
    reason text NOT NULL CHECK (char_length(reason) <= 1000),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (prescription_id, scheduled_on, scheduled_time)
);
CREATE INDEX adherence_patient_date_idx
    ON adherence_marks(patient_id, scheduled_on DESC, scheduled_time DESC);

CREATE TABLE adverse_events (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    prescription_id uuid REFERENCES prescriptions(id),
    author_id uuid NOT NULL REFERENCES users(id),
    occurred_at timestamptz NOT NULL,
    severity text NOT NULL CHECK (severity IN ('mild','moderate','severe')),
    description text NOT NULL CHECK (char_length(description) BETWEEN 1 AND 4000),
    status text NOT NULL CHECK (status IN ('reported','reviewed')),
    review_note text NOT NULL CHECK (char_length(review_note) <= 4000),
    reviewed_by uuid REFERENCES users(id),
    version integer NOT NULL CHECK (version >= 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    reviewed_at timestamptz,
    CHECK ((status = 'reported' AND reviewed_by IS NULL AND reviewed_at IS NULL AND review_note = '')
        OR (status = 'reviewed' AND reviewed_by IS NOT NULL AND reviewed_at IS NOT NULL))
);
CREATE INDEX adverse_event_patient_time_idx
    ON adverse_events(patient_id, occurred_at DESC, id DESC);

CREATE TABLE adverse_event_changes (
    id uuid PRIMARY KEY,
    adverse_event_id uuid NOT NULL REFERENCES adverse_events(id),
    author_id uuid NOT NULL REFERENCES users(id),
    version integer NOT NULL,
    action text NOT NULL CHECK (action IN ('reported','reviewed')),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (adverse_event_id, version)
);

ALTER TABLE notifications DROP CONSTRAINT notifications_kind_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_kind_check
    CHECK (kind IN ('diary_entry','published_event','prescription_published','adverse_event'));
