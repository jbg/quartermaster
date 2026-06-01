CREATE TABLE IF NOT EXISTS meal_plan (
    id                    TEXT PRIMARY KEY,
    household_id          TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    title                 TEXT NOT NULL,
    status                TEXT NOT NULL,
    constraints_json      TEXT NOT NULL,
    ai_task_id            TEXT REFERENCES ai_task(id) ON DELETE SET NULL,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    created_by            TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by            TEXT REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_meal_plan_household_time
    ON meal_plan(household_id, updated_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS meal_plan_day (
    id                    TEXT PRIMARY KEY,
    household_id          TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    meal_plan_id          TEXT NOT NULL REFERENCES meal_plan(id) ON DELETE CASCADE,
    plan_date             TEXT NOT NULL,
    sort_order            INTEGER NOT NULL,
    created_at            TEXT NOT NULL,
    UNIQUE(meal_plan_id, plan_date)
);

CREATE INDEX IF NOT EXISTS idx_meal_plan_day_plan
    ON meal_plan_day(meal_plan_id, sort_order ASC, plan_date ASC);

CREATE TABLE IF NOT EXISTS meal_plan_meal (
    id                    TEXT PRIMARY KEY,
    household_id          TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    meal_plan_id          TEXT NOT NULL REFERENCES meal_plan(id) ON DELETE CASCADE,
    meal_plan_day_id      TEXT NOT NULL REFERENCES meal_plan_day(id) ON DELETE CASCADE,
    plan_date             TEXT NOT NULL,
    slot_key              TEXT NOT NULL,
    slot_label            TEXT NOT NULL,
    sort_order            INTEGER NOT NULL,
    recipe_id             TEXT REFERENCES recipe(id) ON DELETE SET NULL,
    recipe_version_id     TEXT REFERENCES recipe_version(id) ON DELETE SET NULL,
    recipe_name           TEXT,
    serving_scale         TEXT NOT NULL,
    status                TEXT NOT NULL,
    preflight_json        TEXT,
    warnings_json         TEXT NOT NULL,
    conflicts_json        TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    UNIQUE(meal_plan_day_id, slot_key)
);

CREATE INDEX IF NOT EXISTS idx_meal_plan_meal_plan
    ON meal_plan_meal(meal_plan_id, plan_date ASC, sort_order ASC);

CREATE INDEX IF NOT EXISTS idx_meal_plan_meal_recipe
    ON meal_plan_meal(household_id, recipe_id);

CREATE TABLE IF NOT EXISTS stock_reservation (
    id                    TEXT PRIMARY KEY,
    household_id          TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    meal_plan_id          TEXT NOT NULL REFERENCES meal_plan(id) ON DELETE CASCADE,
    meal_plan_meal_id     TEXT NOT NULL REFERENCES meal_plan_meal(id) ON DELETE CASCADE,
    batch_id              TEXT NOT NULL REFERENCES stock_batch(id) ON DELETE CASCADE,
    product_id            TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    quantity              TEXT NOT NULL,
    unit                  TEXT NOT NULL,
    status                TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stock_reservation_active_batch
    ON stock_reservation(household_id, batch_id, status);

CREATE INDEX IF NOT EXISTS idx_stock_reservation_plan_meal
    ON stock_reservation(meal_plan_id, meal_plan_meal_id, status);

ALTER TABLE recipe_execution ADD COLUMN meal_plan_id TEXT REFERENCES meal_plan(id) ON DELETE SET NULL;
ALTER TABLE recipe_execution ADD COLUMN meal_plan_meal_id TEXT REFERENCES meal_plan_meal(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_recipe_execution_meal_plan
    ON recipe_execution(household_id, meal_plan_id, meal_plan_meal_id);
