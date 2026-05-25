CREATE TABLE IF NOT EXISTS recipe (
    id                  TEXT PRIMARY KEY,
    household_id        TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    description         TEXT,
    serving_count       TEXT NOT NULL,
    source              TEXT NOT NULL,
    visibility          TEXT NOT NULL,
    tags_json           TEXT NOT NULL,
    latest_version_id   TEXT,
    created_by          TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by          TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recipe_household
    ON recipe(household_id, name);

CREATE TABLE IF NOT EXISTS recipe_version (
    id                TEXT PRIMARY KEY,
    household_id      TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    recipe_id         TEXT NOT NULL REFERENCES recipe(id) ON DELETE CASCADE,
    version_number    INTEGER NOT NULL,
    serving_count     TEXT NOT NULL,
    source_text       TEXT,
    payload_json      TEXT NOT NULL,
    created_by        TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at        TEXT NOT NULL,
    UNIQUE(recipe_id, version_number)
);

CREATE INDEX IF NOT EXISTS idx_recipe_version_recipe
    ON recipe_version(recipe_id, version_number DESC);

CREATE TABLE IF NOT EXISTS recipe_ingredient (
    id                       TEXT PRIMARY KEY,
    household_id             TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    recipe_id                TEXT NOT NULL REFERENCES recipe(id) ON DELETE CASCADE,
    recipe_version_id        TEXT NOT NULL REFERENCES recipe_version(id) ON DELETE CASCADE,
    sort_order               INTEGER NOT NULL,
    ingredient_id            TEXT REFERENCES ingredient(id) ON DELETE SET NULL,
    product_id               TEXT REFERENCES product(id) ON DELETE SET NULL,
    display_name             TEXT NOT NULL,
    amount                   TEXT,
    unit                     TEXT,
    family                   TEXT,
    range_min                TEXT,
    range_max                TEXT,
    to_taste                 INTEGER NOT NULL DEFAULT 0,
    preparation              TEXT,
    optional                 INTEGER NOT NULL DEFAULT 0,
    group_label              TEXT,
    substitution_hints_json  TEXT NOT NULL,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recipe_ingredient_version
    ON recipe_ingredient(recipe_version_id, sort_order);

CREATE INDEX IF NOT EXISTS idx_recipe_ingredient_ingredient
    ON recipe_ingredient(ingredient_id);

CREATE INDEX IF NOT EXISTS idx_recipe_ingredient_product
    ON recipe_ingredient(product_id);

CREATE TABLE IF NOT EXISTS recipe_step (
    id                       TEXT PRIMARY KEY,
    household_id             TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    recipe_id                TEXT NOT NULL REFERENCES recipe(id) ON DELETE CASCADE,
    recipe_version_id        TEXT NOT NULL REFERENCES recipe_version(id) ON DELETE CASCADE,
    sort_order               INTEGER NOT NULL,
    instruction              TEXT NOT NULL,
    timers_json              TEXT NOT NULL,
    equipment_json           TEXT NOT NULL,
    ingredient_refs_json     TEXT NOT NULL,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recipe_step_version
    ON recipe_step(recipe_version_id, sort_order);

CREATE TABLE IF NOT EXISTS recipe_output (
    id                       TEXT PRIMARY KEY,
    household_id             TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    recipe_id                TEXT NOT NULL REFERENCES recipe(id) ON DELETE CASCADE,
    recipe_version_id        TEXT NOT NULL REFERENCES recipe_version(id) ON DELETE CASCADE,
    sort_order               INTEGER NOT NULL,
    product_id               TEXT REFERENCES product(id) ON DELETE SET NULL,
    name                     TEXT NOT NULL,
    amount                   TEXT,
    unit                     TEXT,
    family                   TEXT,
    range_min                TEXT,
    range_max                TEXT,
    to_taste                 INTEGER NOT NULL DEFAULT 0,
    preparation_note         TEXT,
    expires_after_days       INTEGER,
    storage_notes            TEXT,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recipe_output_version
    ON recipe_output(recipe_version_id, sort_order);

CREATE TABLE IF NOT EXISTS recipe_provenance (
    id                   TEXT PRIMARY KEY,
    household_id         TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    recipe_id            TEXT NOT NULL REFERENCES recipe(id) ON DELETE CASCADE,
    recipe_version_id    TEXT NOT NULL REFERENCES recipe_version(id) ON DELETE CASCADE,
    source_type          TEXT NOT NULL,
    imported_url         TEXT,
    imported_file_name   TEXT,
    imported_text        TEXT,
    prompt_version       TEXT,
    model                TEXT,
    user_edits_json      TEXT NOT NULL,
    parser_confidence    TEXT,
    created_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recipe_provenance_version
    ON recipe_provenance(recipe_version_id);
