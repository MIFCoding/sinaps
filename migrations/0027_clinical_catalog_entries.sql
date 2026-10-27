CREATE TABLE clinical_entry_draft_specs (
    draft_id uuid PRIMARY KEY REFERENCES catalog_drafts(id),
    spec jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE clinical_catalog_entry_versions (
    id uuid PRIMARY KEY,
    draft_id uuid NOT NULL REFERENCES catalog_drafts(id),
    draft_version integer NOT NULL CHECK (draft_version > 0),
    kind text NOT NULL CHECK (kind IN ('disease', 'finding')),
    code text,
    title text,
    label text,
    context text,
    source_code text,
    source_page integer,
    source text NOT NULL CHECK (char_length(source) BETWEEN 1 AND 2000),
    active boolean NOT NULL DEFAULT true,
    published_by uuid NOT NULL REFERENCES users(id),
    published_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(draft_id, draft_version),
    CHECK ((kind = 'disease' AND code IS NOT NULL AND title IS NOT NULL AND label IS NULL AND source_code IS NULL)
        OR (kind = 'finding' AND code IS NULL AND title IS NULL AND label IS NOT NULL AND context IS NOT NULL AND source_code IS NOT NULL AND source_page IS NOT NULL)),
    CHECK (source_page IS NULL OR source_page > 0)
);
CREATE UNIQUE INDEX clinical_catalog_entry_one_active_idx ON clinical_catalog_entry_versions(draft_id) WHERE active;
CREATE UNIQUE INDEX clinical_catalog_disease_code_active_idx ON clinical_catalog_entry_versions(code) WHERE active AND kind = 'disease';
CREATE INDEX clinical_catalog_entries_active_idx ON clinical_catalog_entry_versions(kind, active, published_at DESC);

CREATE TABLE clinical_catalog_entry_changes (
    id uuid PRIMARY KEY,
    entry_version_id uuid NOT NULL REFERENCES clinical_catalog_entry_versions(id),
    author_id uuid NOT NULL REFERENCES users(id),
    action text NOT NULL CHECK (action IN ('published', 'replaced')),
    created_at timestamptz NOT NULL DEFAULT now()
);
