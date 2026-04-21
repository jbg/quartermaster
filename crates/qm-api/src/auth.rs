use argon2::{
    password_hash::{rand_core::OsRng as ArgonOsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{error::ApiError, AppState};

pub fn hash_password(plaintext: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut ArgonOsRng);
    Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("hash_password: {e}")))
}

pub fn verify_password(plaintext: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default()
        .verify_password(plaintext.as_bytes(), &parsed)
        .is_ok()
}

/// Generates a 32-byte random value as a URL-safe base64 string.
pub fn generate_token() -> String {
    use argon2::password_hash::rand_core::RngCore;
    let mut bytes = [0u8; 32];
    ArgonOsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn sha256_hex(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub user_id: Uuid,
    pub household_id: Option<Uuid>,
    pub role: Option<String>,
}

impl<S> FromRequestParts<S> for CurrentUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let bearer = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;

        let hash = sha256_hex(bearer);
        let token = qm_db::tokens::find_active_by_hash(&app_state.db, &hash)
            .await?
            .ok_or(ApiError::Unauthorized)?;

        if token.kind != qm_db::tokens::KIND_ACCESS {
            return Err(ApiError::Unauthorized);
        }

        let expires: DateTime<Utc> = DateTime::parse_from_rfc3339(&token.expires_at)
            .map_err(|_| ApiError::Unauthorized)?
            .with_timezone(&Utc);
        if expires < Utc::now() {
            return Err(ApiError::Unauthorized);
        }

        qm_db::tokens::touch_last_used(&app_state.db, token.id).await?;

        let household = qm_db::households::find_for_user(&app_state.db, token.user_id).await?;
        let role = if let Some(household) = household.as_ref() {
            qm_db::memberships::find(&app_state.db, household.id, token.user_id)
                .await?
                .map(|m| m.role)
        } else {
            None
        };

        Ok(CurrentUser {
            user_id: token.user_id,
            household_id: household.map(|h| h.id),
            role,
        })
    }
}
