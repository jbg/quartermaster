CREATE TABLE hosted_identity_email_required_check (email TEXT NOT NULL);

INSERT INTO hosted_identity_email_required_check (email)
SELECT email
FROM users;

DROP TABLE hosted_identity_email_required_check;

ALTER TABLE users ADD COLUMN display_name TEXT;

UPDATE users
SET email = LOWER(email)
WHERE email IS NOT NULL;

UPDATE users
SET display_name = username
WHERE display_name IS NULL OR display_name = '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_unique ON users(email);

UPDATE membership
SET role = 'read_write'
WHERE role = 'member';

UPDATE invite
SET role_granted = 'read_write'
WHERE role_granted = 'member';

ALTER TABLE barcode_cache ADD COLUMN household_id TEXT REFERENCES household(id) ON DELETE CASCADE;

UPDATE barcode_cache
SET household_id = (
    SELECT created_by_household_id
    FROM product
    WHERE product.id = barcode_cache.product_id
)
WHERE household_id IS NULL
  AND product_id IS NOT NULL;

DELETE FROM barcode_cache
WHERE household_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_barcode_cache_household_barcode
    ON barcode_cache(household_id, barcode);

CREATE INDEX IF NOT EXISTS idx_product_household_barcode
    ON product(created_by_household_id, off_barcode);
