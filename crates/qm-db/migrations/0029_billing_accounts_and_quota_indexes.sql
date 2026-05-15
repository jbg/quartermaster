CREATE TABLE IF NOT EXISTS billing_account (
    id          TEXT PRIMARY KEY,
    plan_key    TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

ALTER TABLE household ADD COLUMN billing_account_id TEXT REFERENCES billing_account(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_household_billing_account
    ON household(billing_account_id);

CREATE INDEX IF NOT EXISTS idx_stock_batch_household_active
    ON stock_batch(household_id, depleted_at);

CREATE INDEX IF NOT EXISTS idx_stock_reminder_household_unacked
    ON stock_reminder(household_id, acked_at);

CREATE INDEX IF NOT EXISTS idx_invite_household_active
    ON invite(household_id, revoked_at, expires_at);
