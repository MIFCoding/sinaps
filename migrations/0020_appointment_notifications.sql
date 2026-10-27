ALTER TABLE notifications
    DROP CONSTRAINT notifications_kind_check;

ALTER TABLE notifications
    ADD CONSTRAINT notifications_kind_check
    CHECK (kind IN (
        'diary_entry',
        'published_event',
        'prescription_published',
        'adverse_event',
        'critical_alert',
        'chat_message',
        'appointment'
    ));
