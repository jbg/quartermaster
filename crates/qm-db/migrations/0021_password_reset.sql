CREATE TABLE IF NOT EXISTS user_password_reset (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash        TEXT NOT NULL,
    token_hash       TEXT NOT NULL,
    expires_at       TEXT NOT NULL,
    consumed_at      TEXT,
    created_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_password_reset_user
    ON user_password_reset(user_id, consumed_at, expires_at);

CREATE INDEX IF NOT EXISTS idx_user_password_reset_code
    ON user_password_reset(user_id, code_hash, consumed_at, expires_at);

CREATE INDEX IF NOT EXISTS idx_user_password_reset_token
    ON user_password_reset(user_id, token_hash, consumed_at, expires_at);
