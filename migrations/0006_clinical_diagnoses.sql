CREATE TABLE clinical_diagnoses (
    id uuid PRIMARY KEY,
    patient_id uuid NOT NULL REFERENCES patient_profiles(user_id),
    code text NOT NULL,
    title text NOT NULL,
    catalog_version text NOT NULL,
    findings jsonb NOT NULL,
    diagnosed_on date NOT NULL,
    status text NOT NULL CHECK (status IN ('active','resolved')),
    note text NOT NULL CHECK (char_length(note) <= 4000),
    version integer NOT NULL CHECK (version >= 1),
    author_id uuid NOT NULL REFERENCES users(id),
    updated_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX clinical_diagnoses_patient_page_idx ON clinical_diagnoses(patient_id,created_at DESC,id DESC);

CREATE TABLE clinical_diagnosis_changes (
    id uuid PRIMARY KEY,
    diagnosis_id uuid NOT NULL REFERENCES clinical_diagnoses(id),
    author_id uuid NOT NULL REFERENCES users(id),
    version integer NOT NULL,
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(diagnosis_id,version)
);
