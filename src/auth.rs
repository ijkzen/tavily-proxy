use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{FromRequestParts, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::{Json, Router, routing::get, routing::post};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::app::AppState;

const SESSION_COOKIE: &str = "tp_session";
const SESSION_TTL_SECS: i64 = 14 * 24 * 3600;
const LOGIN_MAX_FAILURES: usize = 5;
const LOGIN_WINDOW_SECS: i64 = 5 * 60;

/// 登录失败限速：同一用户名在时间窗内失败超限即锁定。
#[derive(Default)]
pub struct LoginRateLimiter {
    failures: std::collections::HashMap<String, std::collections::VecDeque<i64>>,
}

impl LoginRateLimiter {
    pub fn is_locked(&mut self, username: &str) -> bool {
        self.prune(username);
        self.failures
            .get(username)
            .is_some_and(|f| f.len() >= LOGIN_MAX_FAILURES)
    }

    pub fn record_failure(&mut self, username: &str) {
        self.prune(username);
        self.failures.entry(username.to_owned()).or_default().push_back(now());
    }

    pub fn record_success(&mut self, username: &str) {
        self.failures.remove(username);
    }

    fn prune(&mut self, username: &str) {
        let cutoff = now() - LOGIN_WINDOW_SECS;
        if let Some(failures) = self.failures.get_mut(username) {
            while failures.front().is_some_and(|&t| t < cutoff) {
                failures.pop_front();
            }
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(setup_status))
        .route("/setup", post(setup))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/password", post(change_password))
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::encode_b64(&random_bytes::<16>())?;
    Ok(
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)?
            .to_string(),
    )
}

fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build()
}

/// 已登录用户。作为 handler 提取器使用，未登录一律 401。
#[allow(dead_code)] // id 在后续票（用量归属）使用
pub struct AuthUser {
    pub id: i64,
    pub username: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        let token = jar
            .get(SESSION_COOKIE)
            .ok_or(StatusCode::UNAUTHORIZED)?
            .value()
            .to_owned();
        let row = sqlx::query_as::<_, (i64, String, i64)>(
            "SELECT u.id, u.username, s.expires_at \
             FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = ?",
        )
        .bind(sha256_hex(&token))
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let (id, username, expires_at) = row.ok_or(StatusCode::UNAUTHORIZED)?;
        if expires_at < now() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(AuthUser { id, username })
    }
}

async fn user_count(db: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await
        .unwrap_or(0)
}

async fn setup_status(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "needs_setup": user_count(&state.db).await == 0 }))
}

#[derive(Deserialize)]
struct SetupRequest {
    username: String,
    password: String,
}

async fn setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> Result<StatusCode, StatusCode> {
    if user_count(&state.db).await > 0 {
        return Err(StatusCode::FORBIDDEN);
    }
    let username = body.username.trim();
    if username.is_empty() || body.password.len() < 8 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let hash = hash_password(&body.password).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO users (username, password_hash, created_at) VALUES (?, ?, ?)")
        .bind(username)
        .bind(hash)
        .bind(now())
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, StatusCode), StatusCode> {
    let username = body.username.trim().to_owned();
    {
        let mut limiter = state.login_limiter.lock().unwrap();
        if limiter.is_locked(&username) {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, password_hash FROM users WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let verified = row
        .filter(|(_, hash)| verify_password(hash, &body.password));
    let Some((user_id, _)) = verified else {
        state.login_limiter.lock().unwrap().record_failure(&username);
        return Err(StatusCode::UNAUTHORIZED);
    };
    state.login_limiter.lock().unwrap().record_success(&username);

    let token = hex::encode(random_bytes::<32>());
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(sha256_hex(&token))
        .bind(user_id)
        .bind(now() + SESSION_TTL_SECS)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((jar.add(session_cookie(token)), StatusCode::OK))
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> (CookieJar, StatusCode) {
    if let Some(token) = jar.get(SESSION_COOKIE) {
        let _ = sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(sha256_hex(token.value()))
            .execute(&state.db)
            .await;
    }
    (jar.remove(session_cookie(String::new())), StatusCode::OK)
}

async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({ "username": user.username }))
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, StatusCode> {
    if body.new_password.len() < 8 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let hash = sqlx::query_scalar::<_, String>("SELECT password_hash FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !verify_password(&hash, &body.current_password) {
        return Err(StatusCode::FORBIDDEN);
    }
    let new_hash =
        hash_password(&body.new_password).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(new_hash)
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // 改密后吊销全部 session，强制重新登录
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
