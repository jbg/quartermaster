-- Access + refresh tokens belong to one login session and must be revoked
-- together on logout. We model that with an explicit session_id shared by
-- the pair and preserved across refresh rotation.

ALTER TABLE auth_token ADD COLUMN session_id TEXT NOT NULL DEFAULT '';
UPDATE auth_token SET session_id = id WHERE session_id = '';
