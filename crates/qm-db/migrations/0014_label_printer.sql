CREATE TABLE IF NOT EXISTS label_printer (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES household(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    driver        TEXT NOT NULL,
    address       TEXT NOT NULL,
    port          INTEGER NOT NULL,
    media         TEXT NOT NULL,
    enabled       INTEGER NOT NULL,
    is_default    INTEGER NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_label_printer_household ON label_printer(household_id);
CREATE INDEX IF NOT EXISTS idx_label_printer_default ON label_printer(household_id, is_default);
