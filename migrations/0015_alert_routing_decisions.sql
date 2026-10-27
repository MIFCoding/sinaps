ALTER TABLE alerts
    ADD COLUMN routing_category text,
    ADD COLUMN routing_comment text NOT NULL DEFAULT '',
    ADD COLUMN routing_decided_at timestamptz,
    ADD COLUMN routing_decided_by uuid REFERENCES users(id);

ALTER TABLE alerts
    ADD CONSTRAINT alerts_routing_category_check
    CHECK (routing_category IS NULL OR routing_category IN ('mandatory', 'preferred', 'not_indicated'));

ALTER TABLE alert_changes
    DROP CONSTRAINT alert_changes_action_check;

ALTER TABLE alert_changes
    ADD CONSTRAINT alert_changes_action_check
    CHECK (action IN ('detected', 'acknowledged', 'resolved', 'routing_decided'));
