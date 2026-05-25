CREATE TABLE IF NOT EXISTS ai_task (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    task_type                  TEXT NOT NULL,
    provider                   TEXT NOT NULL,
    model                      TEXT,
    prompt_version             TEXT NOT NULL,
    input_digest               TEXT NOT NULL,
    input_summary_json         TEXT NOT NULL,
    output_json                TEXT,
    validation_status          TEXT NOT NULL,
    validation_errors_json     TEXT NOT NULL,
    user_state                 TEXT NOT NULL,
    credentials_assertion      INTEGER NOT NULL DEFAULT 1,
    raw_response_json          TEXT,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ai_task_household_time
    ON ai_task(household_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_task_type
    ON ai_task(household_id, task_type, created_at DESC);
