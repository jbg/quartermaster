CREATE TABLE IF NOT EXISTS replenishment_rule (
    id                            TEXT PRIMARY KEY,
    household_id                  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    product_id                    TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    location_id                   TEXT REFERENCES location(id) ON DELETE SET NULL,
    minimum_quantity              TEXT NOT NULL,
    target_quantity               TEXT NOT NULL,
    unit                          TEXT NOT NULL,
    preferred_supplier_id         TEXT REFERENCES supplier(id) ON DELETE SET NULL,
    preferred_supplier_item_id    TEXT,
    preferred_package_quantity    TEXT,
    preferred_package_unit        TEXT,
    automation_level              TEXT NOT NULL,
    expiry_suppression_days       INTEGER,
    paused_at                     TEXT,
    pause_reason                  TEXT,
    spend_cap_amount              TEXT,
    spend_cap_currency            TEXT,
    created_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                    TEXT NOT NULL,
    updated_at                    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_replenishment_rule_household_product
    ON replenishment_rule(household_id, product_id, location_id);

CREATE INDEX IF NOT EXISTS idx_replenishment_rule_supplier
    ON replenishment_rule(household_id, preferred_supplier_id);

CREATE TABLE IF NOT EXISTS replenishment_settings (
    household_id                  TEXT PRIMARY KEY REFERENCES household(id) ON DELETE CASCADE,
    global_disabled               INTEGER NOT NULL DEFAULT 0,
    default_spend_cap_amount      TEXT,
    default_spend_cap_currency    TEXT,
    notification_lead_minutes     INTEGER NOT NULL DEFAULT 0,
    quiet_hours_start             TEXT,
    quiet_hours_end               TEXT,
    updated_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                    TEXT NOT NULL,
    updated_at                    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS replenishment_supplier_policy (
    id                            TEXT PRIMARY KEY,
    household_id                  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    supplier_id                   TEXT NOT NULL REFERENCES supplier(id) ON DELETE CASCADE,
    disabled                      INTEGER NOT NULL DEFAULT 0,
    spend_cap_amount              TEXT,
    spend_cap_currency            TEXT,
    quiet_hours_start             TEXT,
    quiet_hours_end               TEXT,
    updated_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                    TEXT NOT NULL,
    updated_at                    TEXT NOT NULL,
    UNIQUE (household_id, supplier_id)
);

CREATE INDEX IF NOT EXISTS idx_replenishment_supplier_policy_household
    ON replenishment_supplier_policy(household_id, supplier_id);

CREATE TABLE IF NOT EXISTS replenishment_demand_signal (
    id                            TEXT PRIMARY KEY,
    household_id                  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    product_id                    TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    location_id                   TEXT REFERENCES location(id) ON DELETE SET NULL,
    signal_type                   TEXT NOT NULL,
    status                        TEXT NOT NULL,
    quantity                      TEXT NOT NULL,
    unit                          TEXT NOT NULL,
    recipe_id                     TEXT REFERENCES recipe(id) ON DELETE SET NULL,
    recipe_version_id             TEXT REFERENCES recipe_version(id) ON DELETE SET NULL,
    desired_on                    TEXT,
    supplier_id                   TEXT REFERENCES supplier(id) ON DELETE SET NULL,
    supplier_item_id              TEXT,
    note                          TEXT,
    metadata_json                 TEXT NOT NULL,
    created_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                    TEXT NOT NULL,
    updated_at                    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_replenishment_demand_signal_active
    ON replenishment_demand_signal(household_id, product_id, status, desired_on);

CREATE TABLE IF NOT EXISTS replenishment_cart_run (
    id                            TEXT PRIMARY KEY,
    household_id                  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    draft_id                      TEXT REFERENCES supplier_cart_draft(id) ON DELETE SET NULL,
    order_id                      TEXT REFERENCES supplier_order(id) ON DELETE SET NULL,
    supplier_id                   TEXT REFERENCES supplier(id) ON DELETE SET NULL,
    status                        TEXT NOT NULL,
    source                        TEXT NOT NULL,
    guardrail_decision            TEXT NOT NULL,
    guardrail_snapshot_json       TEXT NOT NULL,
    recommendations_json          TEXT NOT NULL,
    suppressions_json             TEXT NOT NULL,
    ai_explanation_json           TEXT,
    created_by                    TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                    TEXT NOT NULL,
    updated_at                    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_replenishment_cart_run_household
    ON replenishment_cart_run(household_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_replenishment_cart_run_draft
    ON replenishment_cart_run(draft_id);
