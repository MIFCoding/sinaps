CREATE TABLE clinical_recommendations (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id) ON DELETE CASCADE,
    source_kind text NOT NULL CHECK (source_kind IN ('diagnosis', 'questionnaire')),
    source_id uuid NOT NULL,
    rule_code text NOT NULL,
    rule_version text NOT NULL,
    title text NOT NULL,
    rationale text NOT NULL,
    suggested_route text NOT NULL CHECK (suggested_route IN ('mandatory', 'preferred')),
    source_url text NOT NULL,
    evidence jsonb NOT NULL,
    status text NOT NULL DEFAULT 'proposed' CHECK (status IN ('proposed', 'accepted', 'declined', 'published', 'withdrawn')),
    decision_comment text NOT NULL DEFAULT '' CHECK (char_length(decision_comment) <= 4000),
    version integer NOT NULL DEFAULT 1 CHECK (version >= 1),
    decided_by uuid REFERENCES users(id),
    decided_at timestamptz,
    published_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_kind, source_id, rule_code, rule_version),
    CHECK (
        (status = 'proposed' AND decided_by IS NULL AND decided_at IS NULL AND published_at IS NULL)
        OR (status IN ('accepted', 'declined') AND decided_by IS NOT NULL AND decided_at IS NOT NULL AND published_at IS NULL)
        OR (status = 'published' AND decided_by IS NOT NULL AND decided_at IS NOT NULL AND published_at IS NOT NULL)
        OR (status = 'withdrawn' AND decided_by IS NOT NULL AND decided_at IS NOT NULL AND published_at IS NULL)
    )
);

CREATE INDEX clinical_recommendations_patient_idx
    ON clinical_recommendations(patient_id, created_at DESC, id DESC);

CREATE TABLE clinical_recommendation_changes (
    id uuid PRIMARY KEY,
    recommendation_id uuid NOT NULL REFERENCES clinical_recommendations(id) ON DELETE CASCADE,
    author_id uuid REFERENCES users(id),
    version integer NOT NULL,
    action text NOT NULL CHECK (action IN ('proposed', 'accepted', 'declined', 'published', 'withdrawn')),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (recommendation_id, version)
);
