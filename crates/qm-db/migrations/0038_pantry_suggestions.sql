CREATE TABLE IF NOT EXISTS pantry_suggestion (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    source                     TEXT NOT NULL,
    status                     TEXT NOT NULL,
    recipe_id                  TEXT REFERENCES recipe(id) ON DELETE SET NULL,
    recipe_version_id          TEXT REFERENCES recipe_version(id) ON DELETE SET NULL,
    ai_task_id                 TEXT REFERENCES ai_task(id) ON DELETE SET NULL,
    title                      TEXT NOT NULL,
    summary                    TEXT,
    score                      INTEGER NOT NULL,
    score_breakdown_json       TEXT NOT NULL,
    missing_json               TEXT NOT NULL,
    pantry_items_json          TEXT NOT NULL,
    generated_recipe_json      TEXT,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pantry_suggestion_household_time
    ON pantry_suggestion(household_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_pantry_suggestion_status
    ON pantry_suggestion(household_id, status, created_at DESC);
