CREATE TABLE questionnaire_draft_specs (
    draft_id uuid PRIMARY KEY REFERENCES catalog_drafts(id),
    spec jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
