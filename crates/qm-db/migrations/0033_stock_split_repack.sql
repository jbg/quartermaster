ALTER TABLE stock_batch ADD COLUMN source_batch_id TEXT REFERENCES stock_batch(id) ON DELETE SET NULL;
ALTER TABLE stock_batch ADD COLUMN source_operation_id TEXT;

CREATE INDEX IF NOT EXISTS idx_stock_batch_source_batch
    ON stock_batch(source_batch_id);

CREATE INDEX IF NOT EXISTS idx_stock_batch_source_operation
    ON stock_batch(source_operation_id);

ALTER TABLE stock_event ADD COLUMN operation_id TEXT;

CREATE INDEX IF NOT EXISTS idx_stock_event_operation
    ON stock_event(operation_id);
