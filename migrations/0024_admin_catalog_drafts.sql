ALTER TABLE users DROP CONSTRAINT users_role_check;
ALTER TABLE users ADD CONSTRAINT users_role_check CHECK (role IN ('doctor', 'patient', 'admin'));
CREATE UNIQUE INDEX users_one_admin_idx ON users ((role)) WHERE role = 'admin';

CREATE TABLE catalog_drafts (
    id uuid PRIMARY KEY,
    kind text NOT NULL CHECK (kind IN ('questionnaire', 'disease', 'finding', 'drug', 'adverse_effect')),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    source text NOT NULL CHECK (char_length(source) <= 2000),
    details text NOT NULL CHECK (char_length(details) <= 20000),
    version integer NOT NULL CHECK (version > 0),
    author_id uuid NOT NULL REFERENCES users(id),
    updated_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX catalog_drafts_updated_idx ON catalog_drafts(updated_at DESC, id DESC);

CREATE TABLE catalog_draft_changes (
    id uuid PRIMARY KEY,
    draft_id uuid NOT NULL REFERENCES catalog_drafts(id),
    author_id uuid NOT NULL REFERENCES users(id),
    version integer NOT NULL CHECK (version > 0),
    snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(draft_id, version)
);
