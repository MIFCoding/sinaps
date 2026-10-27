ALTER TABLE users
ADD COLUMN activated boolean NOT NULL DEFAULT true;

ALTER TABLE patient_profiles
ADD COLUMN version integer NOT NULL DEFAULT 1 CHECK (version > 0);

CREATE TABLE
    patient_invitations (
        id uuid PRIMARY KEY,
        patient_id uuid NOT NULL REFERENCES patient_profiles (user_id),
        created_by uuid NOT NULL REFERENCES users (id),
        token_hash text NOT NULL UNIQUE,
        expires_at timestamptz NOT NULL,
        consumed_at timestamptz,
        revoked_at timestamptz,
        created_at timestamptz NOT NULL DEFAULT now ()
    );

CREATE INDEX invitation_patient_idx ON patient_invitations (patient_id, created_at DESC);

CREATE UNIQUE INDEX one_pending_invitation_idx ON patient_invitations (patient_id)
WHERE
    consumed_at IS NULL
    AND revoked_at IS NULL;

CREATE TABLE
    patient_profile_changes (
        id uuid PRIMARY KEY,
        patient_id uuid NOT NULL REFERENCES patient_profiles (user_id),
        author_id uuid NOT NULL REFERENCES users (id),
        version_before integer NOT NULL,
        version_after integer NOT NULL,
        before_data jsonb NOT NULL,
        after_data jsonb NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now (),
        UNIQUE (patient_id, version_after),
        CHECK (version_after = version_before + 1)
    );

CREATE INDEX profile_changes_patient_idx ON patient_profile_changes (patient_id, created_at DESC);