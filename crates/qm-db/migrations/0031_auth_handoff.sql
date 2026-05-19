CREATE TABLE auth_handoff_request (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_session_id TEXT NOT NULL REFERENCES auth_session(session_id) ON DELETE CASCADE,
    active_household_id TEXT REFERENCES household(id) ON DELETE SET NULL,
    target_device_label TEXT,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    cancelled_at TEXT,
    accepted_session_id TEXT
);

CREATE INDEX idx_auth_handoff_request_user_id
    ON auth_handoff_request(user_id);

CREATE INDEX idx_auth_handoff_request_source_session
    ON auth_handoff_request(source_session_id);
