ALTER TABLE household ADD COLUMN timezone TEXT NOT NULL DEFAULT 'UTC';

ALTER TABLE stock_reminder ADD COLUMN household_timezone TEXT NOT NULL DEFAULT 'UTC';
ALTER TABLE stock_reminder ADD COLUMN expires_on TEXT;
ALTER TABLE stock_reminder ADD COLUMN household_fire_local_at TEXT;
ALTER TABLE stock_reminder ADD COLUMN acked_at TEXT;

DROP INDEX IF EXISTS idx_stock_reminder_due;
CREATE INDEX IF NOT EXISTS idx_stock_reminder_due
    ON stock_reminder(household_id, kind, acked_at, fire_at, id);

DROP INDEX IF EXISTS idx_stock_reminder_batch_kind;
CREATE INDEX IF NOT EXISTS idx_stock_reminder_batch_kind
    ON stock_reminder(batch_id, kind, acked_at);

UPDATE stock_reminder
SET household_timezone = 'UTC'
WHERE household_timezone = '';

CREATE TABLE IF NOT EXISTS notification_device (
    id                     TEXT PRIMARY KEY,
    user_id                TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id             TEXT NOT NULL REFERENCES auth_session(session_id) ON DELETE CASCADE,
    device_id              TEXT NOT NULL,
    platform               TEXT NOT NULL,
    push_token             TEXT,
    push_authorization     TEXT NOT NULL,
    app_version            TEXT,
    last_seen_at           TEXT NOT NULL,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE(session_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_notification_device_user
    ON notification_device(user_id, platform, updated_at);

CREATE TABLE IF NOT EXISTS reminder_delivery (
    id             TEXT PRIMARY KEY,
    reminder_id    TEXT NOT NULL REFERENCES stock_reminder(id) ON DELETE CASCADE,
    device_id      TEXT REFERENCES notification_device(id) ON DELETE SET NULL,
    channel        TEXT NOT NULL,
    status         TEXT NOT NULL,
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reminder_delivery_reminder
    ON reminder_delivery(reminder_id, channel, created_at);
