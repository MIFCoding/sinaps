CREATE TABLE
    users (
        id uuid PRIMARY KEY,
        email text NOT NULL UNIQUE CHECK (email = lower(email)),
        name text NOT NULL,
        role text NOT NULL CHECK (role IN ('doctor', 'patient')),
        password_hash text NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now ()
    );

CREATE TABLE
    patient_profiles (
        user_id uuid PRIMARY KEY REFERENCES users (id),
        doctor_id uuid NOT NULL REFERENCES users (id),
        birth_date date NOT NULL,
        observation_started date NOT NULL DEFAULT current_date,
        status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'observation', 'archived')),
        medical_summary text NOT NULL DEFAULT '',
        allergies text,
        CHECK (user_id <> doctor_id)
    );

CREATE INDEX patient_doctor_idx ON patient_profiles (doctor_id, status);

CREATE TABLE
    sessions (
        token_hash text PRIMARY KEY,
        user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        expires_at timestamptz NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now ()
    );

CREATE INDEX session_expiry_idx ON sessions (expires_at);

CREATE TABLE
    diary_entries (
        id uuid PRIMARY KEY,
        patient_id uuid NOT NULL REFERENCES patient_profiles (user_id),
        measured_at timestamptz NOT NULL,
        mood smallint NOT NULL CHECK (mood BETWEEN 0 AND 10),
        anxiety smallint NOT NULL CHECK (anxiety BETWEEN 0 AND 10),
        sleep_hours double precision NOT NULL CHECK (sleep_hours BETWEEN 0 AND 24),
        comment text NOT NULL CHECK (char_length(comment) <= 2000),
        created_at timestamptz NOT NULL DEFAULT now ()
    );

CREATE INDEX diary_patient_time_idx ON diary_entries (patient_id, measured_at DESC);

CREATE TABLE
    observation_events (
        id uuid PRIMARY KEY,
        patient_id uuid NOT NULL REFERENCES patient_profiles (user_id),
        author_id uuid NOT NULL REFERENCES users (id),
        title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 120),
        body text NOT NULL CHECK (char_length(body) <= 4000),
        patient_visible boolean NOT NULL DEFAULT false,
        occurred_at timestamptz NOT NULL,
        created_at timestamptz NOT NULL DEFAULT now ()
    );

CREATE INDEX event_patient_time_idx ON observation_events (patient_id, occurred_at DESC);