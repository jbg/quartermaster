CREATE TABLE IF NOT EXISTS ingredient (
    id                  TEXT PRIMARY KEY,
    household_id        TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    display_name        TEXT NOT NULL,
    category            TEXT,
    default_family      TEXT,
    aliases_json        TEXT NOT NULL,
    dietary_tags_json   TEXT NOT NULL,
    allergen_tags_json  TEXT NOT NULL,
    notes               TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ingredient_household
    ON ingredient(household_id);

CREATE INDEX IF NOT EXISTS idx_ingredient_household_name
    ON ingredient(household_id, display_name);

CREATE TABLE IF NOT EXISTS ingredient_product_mapping (
    id                           TEXT PRIMARY KEY,
    household_id                 TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    ingredient_id                TEXT NOT NULL REFERENCES ingredient(id) ON DELETE CASCADE,
    product_id                   TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    rank                         INTEGER NOT NULL,
    match_kind                   TEXT NOT NULL,
    match_metadata_json          TEXT NOT NULL,
    recipe_amount                TEXT,
    recipe_unit                  TEXT,
    recipe_family                TEXT,
    recipe_range_min             TEXT,
    recipe_range_max             TEXT,
    recipe_to_taste              INTEGER NOT NULL DEFAULT 0,
    recipe_preparation_note      TEXT,
    inventory_amount             TEXT,
    inventory_unit               TEXT,
    inventory_family             TEXT,
    inventory_range_min          TEXT,
    inventory_range_max          TEXT,
    inventory_to_taste           INTEGER NOT NULL DEFAULT 0,
    inventory_preparation_note   TEXT,
    conversion_provenance        TEXT,
    conversion_notes             TEXT,
    created_at                   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ingredient_product_mapping_ingredient
    ON ingredient_product_mapping(ingredient_id, rank);

CREATE INDEX IF NOT EXISTS idx_ingredient_product_mapping_product
    ON ingredient_product_mapping(product_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ingredient_product_mapping_unique
    ON ingredient_product_mapping(ingredient_id, product_id);

CREATE TABLE IF NOT EXISTS product_recipe_metadata (
    household_id                  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    product_id                    TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    edible_yield_percent          TEXT,
    drained_quantity              TEXT,
    drained_unit                  TEXT,
    density_recipe_quantity       TEXT,
    density_recipe_unit           TEXT,
    density_inventory_quantity    TEXT,
    density_inventory_unit        TEXT,
    density_provenance            TEXT,
    preparation_state             TEXT,
    counts_as_aliases_json        TEXT NOT NULL,
    notes                         TEXT,
    updated_at                    TEXT NOT NULL,
    PRIMARY KEY (household_id, product_id)
);

CREATE INDEX IF NOT EXISTS idx_product_recipe_metadata_product
    ON product_recipe_metadata(product_id);
