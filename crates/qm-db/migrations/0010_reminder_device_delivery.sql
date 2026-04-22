CREATE TABLE IF NOT EXISTS reminder_device_state (
    reminder_id              TEXT NOT NULL REFERENCES stock_reminder(id) ON DELETE CASCADE,
    device_id                TEXT NOT NULL REFERENCES notification_device(id) ON DELETE CASCADE,
    first_push_attempted_at  TEXT,
    last_push_attempted_at   TEXT,
    last_push_status         TEXT,
    last_push_token          TEXT,
    next_retry_at            TEXT,
    last_error_code          TEXT,
    last_error_message       TEXT,
    first_presented_at       TEXT,
    opened_at                TEXT,
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL,
    PRIMARY KEY (reminder_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_reminder_device_state_device
    ON reminder_device_state(device_id, updated_at);

CREATE INDEX IF NOT EXISTS idx_reminder_device_state_retry
    ON reminder_device_state(last_push_status, next_retry_at, updated_at);

ALTER TABLE reminder_delivery ADD COLUMN attempted_at TEXT;
ALTER TABLE reminder_delivery ADD COLUMN finished_at TEXT;
ALTER TABLE reminder_delivery ADD COLUMN claim_until TEXT;
ALTER TABLE reminder_delivery ADD COLUMN provider_message_id TEXT;
ALTER TABLE reminder_delivery ADD COLUMN error_code TEXT;
ALTER TABLE reminder_delivery ADD COLUMN error_message TEXT;

UPDATE reminder_delivery
SET attempted_at = created_at
WHERE attempted_at IS NULL;

DROP INDEX IF EXISTS idx_reminder_delivery_reminder;
CREATE INDEX IF NOT EXISTS idx_reminder_delivery_reminder
    ON reminder_delivery(reminder_id, device_id, channel, attempted_at);

CREATE INDEX IF NOT EXISTS idx_reminder_delivery_active_claim
    ON reminder_delivery(status, claim_until, channel, reminder_id, device_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_reminder_delivery_success_once
    ON reminder_delivery(reminder_id, device_id, channel)
    WHERE status = 'succeeded';

CREATE UNIQUE INDEX IF NOT EXISTS idx_reminder_delivery_active_once
    ON reminder_delivery(reminder_id, device_id, channel)
    WHERE status = 'sending';
