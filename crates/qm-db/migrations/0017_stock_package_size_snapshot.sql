ALTER TABLE stock_batch ADD COLUMN package_quantity TEXT;
ALTER TABLE stock_batch ADD COLUMN package_unit TEXT;

ALTER TABLE stock_event ADD COLUMN package_quantity TEXT;
ALTER TABLE stock_event ADD COLUMN package_unit TEXT;

UPDATE stock_batch
SET package_quantity = (
        SELECT product.package_quantity
        FROM product
        WHERE product.id = stock_batch.product_id
    ),
    package_unit = (
        SELECT product.package_unit
        FROM product
        WHERE product.id = stock_batch.product_id
    );

UPDATE stock_event
SET package_quantity = (
        SELECT stock_batch.package_quantity
        FROM stock_batch
        WHERE stock_batch.id = stock_event.batch_id
    ),
    package_unit = (
        SELECT stock_batch.package_unit
        FROM stock_batch
        WHERE stock_batch.id = stock_event.batch_id
    );
