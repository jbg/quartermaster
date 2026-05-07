CREATE TABLE IF NOT EXISTS off_credentials (
    user_id             TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    off_username        TEXT NOT NULL,
    encrypted_password  TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

ALTER TABLE product ADD COLUMN off_name TEXT;
ALTER TABLE product ADD COLUMN off_brand TEXT;
ALTER TABLE product ADD COLUMN off_package_quantity TEXT;
ALTER TABLE product ADD COLUMN off_package_unit TEXT;
ALTER TABLE product ADD COLUMN name_local_override INTEGER NOT NULL DEFAULT 0;
ALTER TABLE product ADD COLUMN brand_local_override INTEGER NOT NULL DEFAULT 0;

UPDATE product
SET off_name = name,
    off_brand = brand,
    off_package_quantity = package_quantity,
    off_package_unit = package_unit
WHERE source = 'openfoodfacts';
