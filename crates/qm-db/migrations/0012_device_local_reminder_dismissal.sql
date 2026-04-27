ALTER TABLE reminder_device_state ADD COLUMN dismissed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_reminder_device_state_dismissed
    ON reminder_device_state(device_id, dismissed_at, updated_at);
