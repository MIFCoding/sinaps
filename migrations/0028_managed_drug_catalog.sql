CREATE TABLE drug_draft_specs (
    draft_id uuid PRIMARY KEY REFERENCES catalog_drafts(id),
    spec jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE drug_catalog_versions (
    id uuid PRIMARY KEY,
    draft_id uuid NOT NULL REFERENCES catalog_drafts(id),
    draft_version integer NOT NULL CHECK (draft_version > 0),
    drug_code text NOT NULL CHECK (drug_code = 'aviandr'),
    spec jsonb NOT NULL,
    active boolean NOT NULL DEFAULT true,
    published_by uuid NOT NULL REFERENCES users(id),
    published_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(draft_id, draft_version)
);
CREATE UNIQUE INDEX drug_catalog_one_active_idx ON drug_catalog_versions(drug_code) WHERE active;
CREATE INDEX drug_catalog_versions_draft_idx ON drug_catalog_versions(draft_id, draft_version DESC);

CREATE TABLE drug_catalog_changes (
    id uuid PRIMARY KEY,
    drug_version_id uuid NOT NULL REFERENCES drug_catalog_versions(id),
    author_id uuid NOT NULL REFERENCES users(id),
    action text NOT NULL CHECK (action IN ('published', 'replaced')),
    created_at timestamptz NOT NULL DEFAULT now()
);
