ALTER TABLE appointments
    ADD COLUMN booking_client_id uuid UNIQUE;
