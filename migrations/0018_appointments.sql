CREATE TABLE appointments (
    id uuid PRIMARY KEY,
    client_id uuid NOT NULL UNIQUE,
    doctor_id uuid NOT NULL REFERENCES users(id),
    patient_id uuid REFERENCES patient_profiles(user_id),
    starts_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL,
    status text NOT NULL DEFAULT 'available'
        CHECK (status IN ('available', 'scheduled', 'cancelled')),
    note text NOT NULL DEFAULT '' CHECK (char_length(note) <= 1000),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (ends_at > starts_at),
    CHECK (ends_at - starts_at <= interval '4 hours')
);

CREATE INDEX appointments_doctor_time_idx
    ON appointments(doctor_id, starts_at, ends_at);
CREATE INDEX appointments_patient_time_idx
    ON appointments(patient_id, starts_at DESC)
    WHERE patient_id IS NOT NULL;
