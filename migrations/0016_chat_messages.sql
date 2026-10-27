CREATE TABLE chat_messages (
    id uuid PRIMARY KEY,
    client_id uuid NOT NULL UNIQUE,
    patient_id uuid NOT NULL REFERENCES patient_profiles (user_id),
    sender_id uuid NOT NULL REFERENCES users (id),
    body text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 4000),
    created_at timestamptz NOT NULL DEFAULT now(),
    read_at timestamptz
);

CREATE INDEX chat_messages_patient_time_idx
    ON chat_messages (patient_id, created_at DESC, id DESC);
