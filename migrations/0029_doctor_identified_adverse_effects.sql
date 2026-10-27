ALTER TABLE adverse_events
    ADD COLUMN effect_items text[] NOT NULL DEFAULT '{}';

ALTER TABLE adverse_event_changes
    DROP CONSTRAINT adverse_event_changes_action_check;
ALTER TABLE adverse_event_changes
    ADD CONSTRAINT adverse_event_changes_action_check
    CHECK (action IN ('reported','reviewed','identified'));
