CREATE TABLE IF NOT EXISTS auth_session (
    session_id           TEXT PRIMARY KEY,
    user_id              TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    active_household_id  TEXT REFERENCES household(id) ON DELETE SET NULL,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_session_user ON auth_session(user_id);
