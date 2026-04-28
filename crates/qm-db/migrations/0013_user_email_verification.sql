ALTER TABLE users ADD COLUMN email_verified_at TEXT;

CREATE TABLE IF NOT EXISTS user_email_verification (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    email        TEXT NOT NULL,
    code_hash    TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    consumed_at  TEXT,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_email_verification_user
    ON user_email_verification(user_id, consumed_at, expires_at);
