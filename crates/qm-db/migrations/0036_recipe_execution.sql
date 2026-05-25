CREATE TABLE IF NOT EXISTS recipe_execution (
    id                    TEXT PRIMARY KEY,
    household_id          TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    recipe_id             TEXT,
    recipe_version_id     TEXT,
    recipe_name           TEXT,
    serving_scale         TEXT NOT NULL,
    idempotency_key       TEXT,
    adjusted_recipe_json  TEXT NOT NULL,
    preflight_json        TEXT NOT NULL,
    consume_request_id    TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    created_by            TEXT NOT NULL REFERENCES users(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_recipe_execution_idempotency
    ON recipe_execution(household_id, idempotency_key);

CREATE INDEX IF NOT EXISTS idx_recipe_execution_household_time
    ON recipe_execution(household_id, created_at DESC);

ALTER TABLE stock_event ADD COLUMN recipe_execution_id TEXT REFERENCES recipe_execution(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_stock_event_recipe_execution
    ON stock_event(recipe_execution_id);
