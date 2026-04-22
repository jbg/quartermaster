CREATE INDEX IF NOT EXISTS idx_auth_token_session ON auth_token(session_id);
CREATE INDEX IF NOT EXISTS idx_auth_token_session_live ON auth_token(session_id, revoked_at, expires_at);
