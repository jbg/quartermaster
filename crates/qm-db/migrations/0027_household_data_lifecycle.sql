ALTER TABLE household ADD COLUMN deletion_requested_at TEXT;
ALTER TABLE household ADD COLUMN deletion_requested_by TEXT REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_household_active
    ON household(deletion_requested_at, id);
