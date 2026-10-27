CREATE TABLE questionnaire_assignments (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    author_id uuid NOT NULL REFERENCES users(id),
    scale_code text NOT NULL,
    scale_version text NOT NULL,
    definition jsonb NOT NULL,
    note text NOT NULL CHECK (char_length(note) <= 2000),
    status text NOT NULL CHECK (status IN ('assigned','completed','cancelled')),
    version integer NOT NULL CHECK (version >= 1),
    answers jsonb,
    score smallint CHECK (score BETWEEN 0 AND 21),
    band text CHECK (band IN ('minimal','mild','moderate','severe')),
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    cancelled_at timestamptz,
    CHECK ((status='completed' AND answers IS NOT NULL AND score IS NOT NULL AND band IS NOT NULL AND completed_at IS NOT NULL AND cancelled_at IS NULL)
        OR (status='assigned' AND answers IS NULL AND score IS NULL AND band IS NULL AND completed_at IS NULL AND cancelled_at IS NULL)
        OR (status='cancelled' AND answers IS NULL AND score IS NULL AND band IS NULL AND completed_at IS NULL AND cancelled_at IS NOT NULL))
);
CREATE UNIQUE INDEX questionnaire_one_pending_idx ON questionnaire_assignments(patient_id,scale_code) WHERE status='assigned';
CREATE INDEX questionnaire_patient_page_idx ON questionnaire_assignments(patient_id,created_at DESC,id DESC);
CREATE TABLE questionnaire_changes (
    id uuid PRIMARY KEY,
    assignment_id uuid NOT NULL REFERENCES questionnaire_assignments(id),
    author_id uuid NOT NULL REFERENCES users(id),
    version integer NOT NULL,
    action text NOT NULL CHECK (action IN ('assigned','completed','cancelled')),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(assignment_id,version)
);
