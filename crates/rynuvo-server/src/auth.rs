use axum::{extract::FromRequestParts, http::request::Parts};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rynuvo_proto::User;
use sha2::{Digest, Sha256};

use crate::{app::AppState, error::AppError, now_ms};

const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("os rng");
    b
}

/// 32 random bytes, base64url without padding (43 chars).
pub fn new_secret() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<32>())
}

pub fn hash(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// Human-friendly code like `ABCD-EFGH-JKLM`.
pub fn new_human_code() -> String {
    let bytes = random_bytes::<12>();
    let chars: Vec<char> = bytes.iter().map(|b| CODE_ALPHABET[(*b % 32) as usize] as char).collect();
    chars.chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

/// Normalize a code the user typed: trim, uppercase, drop `-` and whitespace.
pub fn normalize_code(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

pub fn code_hash(code: &str) -> [u8; 32] {
    hash(&normalize_code(code))
}

/// Validate a nickname. Returns (trimmed display name, lowercase uniqueness key).
pub fn nickname_key(n: &str) -> Result<(String, String), AppError> {
    let t = n.trim();
    let len = t.chars().count();
    if !(2..=32).contains(&len) {
        return Err(AppError::BadRequest("Nickname must be 2–32 characters".into()));
    }
    if t.chars().any(|c| c.is_control()) {
        return Err(AppError::BadRequest("Nickname contains invalid characters".into()));
    }
    Ok((t.to_string(), t.to_lowercase()))
}

/// Authenticated caller, resolved from `Authorization: Bearer <token>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    pub device_id: i64,
}

impl AuthUser {
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.user.is_admin { Ok(()) } else { Err(AppError::Forbidden) }
    }
}

/// Look up an active user by raw device token.
pub async fn user_by_token(db: &sqlx::SqlitePool, token: &str) -> Result<Option<AuthUser>, AppError> {
    let h = hash(token);
    let row: Option<(i64, i64, String, bool, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT d.id, u.id, u.nickname, u.is_admin, u.avatar_icon, u.avatar_color FROM devices d JOIN users u ON u.id = d.user_id
         WHERE d.token_hash = ? AND u.removed_at IS NULL",
    )
    .bind(&h[..])
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(device_id, id, nickname, is_admin, avatar_icon, avatar_color)| AuthUser {
        user: User { id, nickname, is_admin, avatar_icon, avatar_color },
        device_id,
    }))
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let au = user_by_token(&state.db, token).await?.ok_or(AppError::Unauthorized)?;
        let _ = sqlx::query("UPDATE devices SET last_seen = ? WHERE id = ?")
            .bind(now_ms())
            .bind(au.device_id)
            .execute(&state.db)
            .await;
        Ok(au)
    }
}
