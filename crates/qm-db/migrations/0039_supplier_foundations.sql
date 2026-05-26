CREATE TABLE IF NOT EXISTS supplier (
    id                         TEXT PRIMARY KEY,
    display_name               TEXT NOT NULL,
    capabilities_json          TEXT NOT NULL,
    requirements_json          TEXT NOT NULL,
    supported_regions_json     TEXT NOT NULL,
    terms_url                  TEXT,
    needs_network              INTEGER NOT NULL DEFAULT 0,
    needs_browser              INTEGER NOT NULL DEFAULT 0,
    enabled                    INTEGER NOT NULL DEFAULT 1,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS supplier_account (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    supplier_id                TEXT NOT NULL REFERENCES supplier(id) ON DELETE CASCADE,
    display_name               TEXT NOT NULL,
    status                     TEXT NOT NULL,
    region_json                TEXT,
    config_json                TEXT NOT NULL,
    consent_accepted_at        TEXT,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_account_household
    ON supplier_account(household_id, supplier_id, created_at DESC);

CREATE TABLE IF NOT EXISTS supplier_account_secret (
    account_id                 TEXT NOT NULL REFERENCES supplier_account(id) ON DELETE CASCADE,
    secret_name                TEXT NOT NULL,
    secret_kind                TEXT NOT NULL,
    encrypted_value            TEXT NOT NULL,
    redacted_hint              TEXT,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL,
    PRIMARY KEY (account_id, secret_name)
);

CREATE TABLE IF NOT EXISTS supplier_catalog_item (
    id                         TEXT PRIMARY KEY,
    supplier_id                TEXT NOT NULL REFERENCES supplier(id) ON DELETE CASCADE,
    supplier_item_id           TEXT NOT NULL,
    name                       TEXT NOT NULL,
    brand                      TEXT,
    image_url                  TEXT,
    detail_url                 TEXT,
    availability               TEXT NOT NULL,
    price_amount               TEXT,
    price_currency             TEXT,
    pack_quantity              TEXT,
    pack_unit                  TEXT,
    lead_time_min_days         INTEGER,
    lead_time_max_days         INTEGER,
    minimum_order_quantity     TEXT,
    minimum_order_unit         TEXT,
    metadata_json              TEXT NOT NULL,
    fetched_at                 TEXT NOT NULL,
    UNIQUE (supplier_id, supplier_item_id)
);

CREATE INDEX IF NOT EXISTS idx_supplier_catalog_item_search
    ON supplier_catalog_item(supplier_id, name);

CREATE TABLE IF NOT EXISTS product_supplier_mapping (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    product_id                 TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    supplier_id                TEXT NOT NULL REFERENCES supplier(id) ON DELETE CASCADE,
    supplier_item_id           TEXT NOT NULL,
    confidence                 TEXT NOT NULL,
    confirmed_at               TEXT,
    substitute_policy_json     TEXT NOT NULL,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_product_supplier_mapping_household_product
    ON product_supplier_mapping(household_id, product_id, supplier_id);

CREATE TABLE IF NOT EXISTS supplier_cart_draft (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    account_id                 TEXT REFERENCES supplier_account(id) ON DELETE SET NULL,
    supplier_id                TEXT NOT NULL REFERENCES supplier(id) ON DELETE CASCADE,
    status                     TEXT NOT NULL,
    source                     TEXT NOT NULL,
    intervention_state         TEXT NOT NULL,
    review_notes              TEXT,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_cart_draft_household_status
    ON supplier_cart_draft(household_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS supplier_cart_line (
    id                         TEXT PRIMARY KEY,
    draft_id                   TEXT NOT NULL REFERENCES supplier_cart_draft(id) ON DELETE CASCADE,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    product_id                 TEXT REFERENCES product(id) ON DELETE SET NULL,
    supplier_item_id           TEXT NOT NULL,
    quantity                   TEXT NOT NULL,
    unit                       TEXT,
    note                       TEXT,
    sort_order                 INTEGER NOT NULL,
    created_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_cart_line_draft
    ON supplier_cart_line(draft_id, sort_order ASC);

CREATE TABLE IF NOT EXISTS supplier_order (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    draft_id                   TEXT REFERENCES supplier_cart_draft(id) ON DELETE SET NULL,
    account_id                 TEXT REFERENCES supplier_account(id) ON DELETE SET NULL,
    supplier_id                TEXT NOT NULL REFERENCES supplier(id) ON DELETE CASCADE,
    supplier_order_id          TEXT,
    status                     TEXT NOT NULL,
    review_url                 TEXT,
    redacted_summary_json      TEXT NOT NULL,
    submitted_at               TEXT,
    delivered_at               TEXT,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_order_household_status
    ON supplier_order(household_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS supplier_order_event (
    id                         TEXT PRIMARY KEY,
    order_id                   TEXT NOT NULL REFERENCES supplier_order(id) ON DELETE CASCADE,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    event_type                 TEXT NOT NULL,
    status                     TEXT,
    redacted_payload_json      TEXT NOT NULL,
    created_at                 TEXT NOT NULL,
    created_by                 TEXT REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_order_event_order
    ON supplier_order_event(order_id, created_at ASC);

CREATE TABLE IF NOT EXISTS supplier_browser_session (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    account_id                 TEXT NOT NULL REFERENCES supplier_account(id) ON DELETE CASCADE,
    status                     TEXT NOT NULL,
    encrypted_cookie_jar       TEXT,
    expires_at                 TEXT,
    created_at                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_browser_session_account
    ON supplier_browser_session(account_id, status, updated_at DESC);

CREATE TABLE IF NOT EXISTS supplier_debug_artifact (
    id                         TEXT PRIMARY KEY,
    household_id               TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    account_id                 TEXT REFERENCES supplier_account(id) ON DELETE SET NULL,
    order_id                   TEXT REFERENCES supplier_order(id) ON DELETE SET NULL,
    artifact_kind              TEXT NOT NULL,
    redacted_body              TEXT NOT NULL,
    content_type               TEXT NOT NULL,
    created_at                 TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supplier_debug_artifact_household
    ON supplier_debug_artifact(household_id, created_at DESC);
