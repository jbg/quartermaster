CREATE TABLE IF NOT EXISTS background_job (
    id             TEXT PRIMARY KEY,
    kind           TEXT NOT NULL,
    dedupe_key     TEXT NOT NULL,
    payload_json   TEXT NOT NULL,
    status         TEXT NOT NULL,
    run_at         TEXT NOT NULL,
    lease_owner    TEXT,
    lease_until    TEXT,
    attempt_count  INTEGER NOT NULL DEFAULT 0,
    max_attempts   INTEGER NOT NULL,
    last_error     TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    finished_at    TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_background_job_active_unique
    ON background_job(kind, dedupe_key)
    WHERE status IN ('pending', 'leased', 'retryable');

CREATE INDEX IF NOT EXISTS idx_background_job_due
    ON background_job(status, run_at, lease_until, id);

CREATE INDEX IF NOT EXISTS idx_background_job_kind_status
    ON background_job(kind, status, updated_at);

ALTER TABLE reminder_delivery ADD COLUMN lease_owner TEXT;

CREATE INDEX IF NOT EXISTS idx_reminder_delivery_lease_owner
    ON reminder_delivery(lease_owner, status, claim_until);
