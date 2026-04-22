CREATE TABLE IF NOT EXISTS stock_reminder (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    batch_id      TEXT NOT NULL REFERENCES stock_batch(id) ON DELETE CASCADE,
    product_id    TEXT NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    location_id   TEXT NOT NULL REFERENCES location(id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,
    fire_at       TEXT NOT NULL,
    title         TEXT NOT NULL,
    body          TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    presented_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_stock_reminder_due
    ON stock_reminder(household_id, kind, presented_at, fire_at, id);

CREATE INDEX IF NOT EXISTS idx_stock_reminder_batch_kind
    ON stock_reminder(batch_id, kind, presented_at);
