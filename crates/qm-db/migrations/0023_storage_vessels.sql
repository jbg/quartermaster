CREATE TABLE IF NOT EXISTS storage_vessel (
    id             TEXT PRIMARY KEY,
    household_id   TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    tare_weight    TEXT NOT NULL,
    tare_unit      TEXT NOT NULL,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_storage_vessel_household
    ON storage_vessel(household_id);

ALTER TABLE stock_batch ADD COLUMN storage_vessel_id TEXT REFERENCES storage_vessel(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_stock_batch_storage_vessel
    ON stock_batch(storage_vessel_id);
