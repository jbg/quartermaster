use argon2::{
    password_hash::{
        rand_core::OsRng as ArgonOsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tracing::Span;
use uuid::Uuid;

use crate::{error::ApiError, AppState};

#[derive(Clone, Debug)]
pub struct ResolvedHousehold {
    pub household_id: Option<Uuid>,
    pub role: Option<String>,
}

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

pub async fn cleanup_session_if_unused(
    db: &qm_db::Database,
    session_id: Uuid,
) -> Result<bool, sqlx::Error> {
    qm_db::auth_sessions::delete_if_no_live_tokens(db, session_id, &qm_db::now_utc_rfc3339()).await
}

#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub household_id: Option<Uuid>,
    pub role: Option<String>,
}

pub async fn resolve_active_household(
    db: &qm_db::Database,
    session_id: Uuid,
    user_id: Uuid,
) -> Result<ResolvedHousehold, sqlx::Error> {
    if let Some(session) = qm_db::auth_sessions::find(db, session_id).await? {
        if session.user_id == user_id {
            if let Some(active_household_id) = session.active_household_id {
                if let Some(membership) =
                    qm_db::memberships::find(db, active_household_id, user_id).await?
                {
                    return Ok(ResolvedHousehold {
                        household_id: Some(active_household_id),
                        role: Some(membership.role),
                    });
                }
            }
        }
    }

    let fallback = qm_db::households::find_for_user(db, user_id).await?;
    let household_id = fallback.as_ref().map(|household| household.id);
    qm_db::auth_sessions::upsert(db, session_id, user_id, household_id).await?;
    let role = if let Some(household_id) = household_id {
        qm_db::memberships::find(db, household_id, user_id)
            .await?
            .map(|membership| membership.role)
    } else {
        None
    };

    Ok(ResolvedHousehold { household_id, role })
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
            cleanup_session_if_unused(&app_state.db, token.session_id).await?;
            return Err(ApiError::Unauthorized);
        }

        let expires: DateTime<Utc> = DateTime::parse_from_rfc3339(&token.expires_at)
            .map_err(|_| ApiError::Unauthorized)?
            .with_timezone(&Utc);
        if expires < Utc::now() {
            cleanup_session_if_unused(&app_state.db, token.session_id).await?;
            return Err(ApiError::Unauthorized);
        }

        qm_db::tokens::touch_last_used(&app_state.db, token.id).await?;
        let resolved =
            resolve_active_household(&app_state.db, token.session_id, token.user_id).await?;

        let span = Span::current();
        span.record("user_id", tracing::field::display(token.user_id));
        if let Some(household_id) = resolved.household_id {
            span.record("household_id", tracing::field::display(household_id));
        }

        Ok(CurrentUser {
            user_id: token.user_id,
            session_id: token.session_id,
            household_id: resolved.household_id,
            role: resolved.role,
        })
    }
}
