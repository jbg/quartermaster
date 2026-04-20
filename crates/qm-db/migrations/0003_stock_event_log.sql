-- Stock becomes event-sourced: the `stock_batch` row is the "account"
-- (immutable initial balance + cached current balance + a tombstone),
-- and `stock_event` is the append-only ledger of every quantity change.
--
-- Source of truth going forward is `stock_event`. `stock_batch.quantity`
-- is a cache maintained transactionally alongside event inserts so that
-- list queries stay O(1); if it ever drifts, the ledger wins and the cache
-- can be rebuilt as SUM(event.quantity_delta) per batch.
--
-- Events are kept forever: the whole point of the ledger is that future
-- consumption-history / analytics features can read their story out of it
-- without schema rework. No retention cap.

CREATE TABLE IF NOT EXISTS stock_event (
    id                   TEXT PRIMARY KEY,
    household_id         TEXT NOT NULL REFERENCES household(id)   ON DELETE CASCADE,
    batch_id             TEXT NOT NULL REFERENCES stock_batch(id) ON DELETE RESTRICT,
    event_type           TEXT NOT NULL,          -- 'add' | 'consume' | 'adjust' | 'discard'
    quantity_delta       TEXT NOT NULL,          -- signed Decimal expressed in the batch's unit
    note                 TEXT,
    created_at           TEXT NOT NULL,
    created_by           TEXT NOT NULL REFERENCES users(id),
    -- Correlates the N rows written by a single POST /stock/consume call
    -- so a household timeline can collapse the fan-out back into one action.
    consume_request_id   TEXT
);

CREATE INDEX IF NOT EXISTS idx_stock_event_batch             ON stock_event(batch_id);
CREATE INDEX IF NOT EXISTS idx_stock_event_household_time    ON stock_event(household_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_stock_event_correlation       ON stock_event(consume_request_id);

-- Immutable record of the amount the batch was first added with, so the
-- ledger is complete for the batch even if the cached quantity is lost.
ALTER TABLE stock_batch ADD COLUMN initial_quantity TEXT NOT NULL DEFAULT '0';
UPDATE stock_batch SET initial_quantity = quantity;

-- Soft-delete for products. `DELETE /products/{id}` sets this and hides the
-- product from catalog queries, but does NOT touch any `stock_batch` or
-- `stock_event` rows that reference it. Depleted batches of a deleted
-- product still resolve their product for history displays.
ALTER TABLE product ADD COLUMN deleted_at TEXT;
