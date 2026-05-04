ALTER TABLE stock_batch ADD COLUMN produced_on TEXT;

CREATE INDEX IF NOT EXISTS idx_stock_batch_produced ON stock_batch(produced_on);
