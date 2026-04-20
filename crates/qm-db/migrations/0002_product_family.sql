-- Products have a fundamental unit family: mass | volume | count.
-- Every batch's unit MUST resolve to the same family as its product's -- so
-- you can never record "4 cups of flour" or "1 kg of milk" at purchase time.
-- Cross-family conversions belong to recipe/consumption logic only.
--
-- The existing `default_unit` column is semantically repurposed as the
-- preferred unit for display — still a valid unit code, just a clearer name
-- in the Rust layer. No rename here to keep the migration trivially portable
-- between SQLite and Postgres.
--
-- DEFAULT 'count' is effectively a no-op backfill: slice 1 shipped with no
-- product rows, so the NOT NULL constraint is never exercised against
-- existing data. The default exists purely so the ALTER is legal.

ALTER TABLE product ADD COLUMN family TEXT NOT NULL DEFAULT 'count';
