ALTER TABLE alerts
    DROP CONSTRAINT alerts_source_kind_check;

ALTER TABLE alerts
    ADD CONSTRAINT alerts_source_kind_check
    CHECK (source_kind IN ('adverse_event', 'questionnaire'));
