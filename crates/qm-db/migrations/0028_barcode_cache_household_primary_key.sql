DROP INDEX IF EXISTS idx_barcode_cache_household_barcode;

ALTER TABLE barcode_cache RENAME TO barcode_cache_old;

CREATE TABLE barcode_cache (
    household_id   TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    barcode        TEXT NOT NULL,
    product_id     TEXT REFERENCES product(id) ON DELETE SET NULL,
    raw_off_json   TEXT,
    fetched_at     TEXT NOT NULL,
    miss           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (household_id, barcode)
);

INSERT INTO barcode_cache (household_id, barcode, product_id, raw_off_json, fetched_at, miss)
SELECT household_id, barcode, product_id, raw_off_json, fetched_at, miss
FROM barcode_cache_old
WHERE household_id IS NOT NULL;

DROP TABLE barcode_cache_old;

CREATE INDEX IF NOT EXISTS idx_barcode_cache_product
    ON barcode_cache(product_id);
