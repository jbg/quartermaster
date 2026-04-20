-- Quartermaster v1 initial schema.
-- Kept in the intersection of SQLite and Postgres syntax so `sqlx::Any` can
-- run the same migration against either backend. In practice that means:
--   * TEXT for UUIDs and timestamps (ISO-8601 UTC)
--   * INTEGER for counters and boolean-like flags (0/1)
--   * no DEFAULT functions (application sets timestamps explicitly)
--   * no backend-specific extensions

CREATE TABLE IF NOT EXISTS household (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id             TEXT PRIMARY KEY,
    username       TEXT NOT NULL UNIQUE,
    email          TEXT,
    password_hash  TEXT NOT NULL,
    created_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS membership (
    household_id  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users(id)     ON DELETE CASCADE,
    role          TEXT NOT NULL,
    joined_at     TEXT NOT NULL,
    PRIMARY KEY (household_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_membership_user ON membership(user_id);

CREATE TABLE IF NOT EXISTS invite (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    code          TEXT NOT NULL UNIQUE,
    created_by    TEXT NOT NULL REFERENCES users(id),
    expires_at    TEXT NOT NULL,
    max_uses      INTEGER NOT NULL,
    use_count     INTEGER NOT NULL DEFAULT 0,
    role_granted  TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invite_household ON invite(household_id);

CREATE TABLE IF NOT EXISTS auth_token (
    id             TEXT PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash     TEXT NOT NULL UNIQUE,
    kind           TEXT NOT NULL,
    device_label   TEXT,
    last_used_at   TEXT NOT NULL,
    expires_at     TEXT NOT NULL,
    revoked_at     TEXT,
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_token_user ON auth_token(user_id);

CREATE TABLE IF NOT EXISTS product (
    id                       TEXT PRIMARY KEY,
    source                   TEXT NOT NULL,
    off_barcode              TEXT UNIQUE,
    name                     TEXT NOT NULL,
    brand                    TEXT,
    default_unit             TEXT NOT NULL,
    image_url                TEXT,
    fetched_at               TEXT,
    created_by_household_id  TEXT REFERENCES household(id) ON DELETE CASCADE,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_product_household ON product(created_by_household_id);

CREATE TABLE IF NOT EXISTS barcode_cache (
    barcode       TEXT PRIMARY KEY,
    product_id    TEXT REFERENCES product(id) ON DELETE SET NULL,
    raw_off_json  TEXT,
    fetched_at    TEXT NOT NULL,
    miss          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS location (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_location_household ON location(household_id);

CREATE TABLE IF NOT EXISTS stock_batch (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    product_id    TEXT NOT NULL REFERENCES product(id),
    location_id   TEXT NOT NULL REFERENCES location(id),
    quantity      TEXT NOT NULL,
    unit          TEXT NOT NULL,
    expires_on    TEXT,
    opened_on     TEXT,
    note          TEXT,
    created_at    TEXT NOT NULL,
    created_by    TEXT NOT NULL REFERENCES users(id),
    depleted_at   TEXT
);

CREATE INDEX IF NOT EXISTS idx_stock_batch_household ON stock_batch(household_id);
CREATE INDEX IF NOT EXISTS idx_stock_batch_product   ON stock_batch(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_batch_location  ON stock_batch(location_id);
CREATE INDEX IF NOT EXISTS idx_stock_batch_expires   ON stock_batch(expires_on);
CREATE INDEX IF NOT EXISTS idx_stock_batch_depleted  ON stock_batch(depleted_at);
