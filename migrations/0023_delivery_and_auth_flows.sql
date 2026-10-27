ALTER TABLE users
    ADD COLUMN email_verified boolean NOT NULL DEFAULT FALSE;

CREATE TABLE auth_action_tokens (
    token_hash text PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose text NOT NULL CHECK (purpose IN ('verify_email', 'reset_password')),
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX auth_action_tokens_user_idx ON auth_action_tokens(user_id, purpose, created_at DESC);
CREATE INDEX auth_action_tokens_expiry_idx ON auth_action_tokens(expires_at);

CREATE TABLE notification_push_outbox (
    notification_id uuid PRIMARY KEY REFERENCES notifications(id) ON DELETE CASCADE,
    attempts integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    lease_until timestamptz,
    delivered_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE push_delivery_settings (
    id smallint PRIMARY KEY CHECK (id = 1),
    enabled boolean NOT NULL DEFAULT FALSE
);

INSERT INTO push_delivery_settings(id, enabled) VALUES (1, FALSE);

CREATE FUNCTION enqueue_notification_push() RETURNS trigger AS $$
BEGIN
    IF (SELECT enabled FROM push_delivery_settings WHERE id = 1) THEN
        INSERT INTO notification_push_outbox(notification_id) VALUES (NEW.id)
        ON CONFLICT (notification_id) DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER notifications_enqueue_push
AFTER INSERT ON notifications
FOR EACH ROW EXECUTE FUNCTION enqueue_notification_push();
