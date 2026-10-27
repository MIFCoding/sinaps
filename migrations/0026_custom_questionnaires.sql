CREATE TABLE custom_questionnaire_versions (
    id uuid PRIMARY KEY,
    draft_id uuid NOT NULL REFERENCES catalog_drafts(id),
    draft_version integer NOT NULL CHECK (draft_version > 0),
    title text NOT NULL,
    source text NOT NULL,
    spec jsonb NOT NULL,
    active boolean NOT NULL DEFAULT true,
    published_by uuid NOT NULL REFERENCES users(id),
    published_at timestamptz NOT NULL DEFAULT now(),
    archived_at timestamptz,
    UNIQUE(draft_id, draft_version),
    CHECK ((active AND archived_at IS NULL) OR (NOT active AND archived_at IS NOT NULL))
);
CREATE UNIQUE INDEX custom_questionnaire_one_active_idx ON custom_questionnaire_versions(draft_id) WHERE active;

CREATE TABLE custom_questionnaire_assignments (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    author_id uuid NOT NULL REFERENCES users(id),
    form_id uuid NOT NULL REFERENCES custom_questionnaire_versions(id),
    draft_id uuid NOT NULL REFERENCES catalog_drafts(id),
    title text NOT NULL,
    source text NOT NULL,
    spec jsonb NOT NULL,
    note text NOT NULL CHECK (char_length(note) <= 2000),
    status text NOT NULL CHECK (status IN ('assigned', 'completed', 'cancelled')),
    version integer NOT NULL CHECK (version > 0),
    answers jsonb,
    score integer,
    band text,
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    cancelled_at timestamptz,
    CHECK ((status='assigned' AND answers IS NULL AND score IS NULL AND band IS NULL AND completed_at IS NULL AND cancelled_at IS NULL)
        OR (status='completed' AND answers IS NOT NULL AND score IS NOT NULL AND completed_at IS NOT NULL AND cancelled_at IS NULL)
        OR (status='cancelled' AND answers IS NULL AND score IS NULL AND band IS NULL AND completed_at IS NULL AND cancelled_at IS NOT NULL))
);
CREATE UNIQUE INDEX custom_questionnaire_one_pending_idx ON custom_questionnaire_assignments(patient_id, draft_id) WHERE status='assigned';
CREATE INDEX custom_questionnaire_patient_idx ON custom_questionnaire_assignments(patient_id, created_at DESC, id DESC);

CREATE TABLE custom_questionnaire_changes (
    id uuid PRIMARY KEY,
    assignment_id uuid NOT NULL REFERENCES custom_questionnaire_assignments(id),
    author_id uuid NOT NULL REFERENCES users(id),
    version integer NOT NULL,
    action text NOT NULL CHECK (action IN ('assigned', 'completed', 'cancelled')),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(assignment_id, version)
);

CREATE TABLE custom_questionnaire_version_changes (
    id uuid PRIMARY KEY,
    form_id uuid NOT NULL REFERENCES custom_questionnaire_versions(id),
    author_id uuid NOT NULL REFERENCES users(id),
    action text NOT NULL CHECK (action IN ('published', 'archived')),
    created_at timestamptz NOT NULL DEFAULT now()
);
