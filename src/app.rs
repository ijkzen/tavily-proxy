use axum::http::StatusCode;
use axum::{Router, routing::get};
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};

use crate::auth::LoginRateLimiter;
use crate::crypto::Crypto;
use crate::{assets, auth, upstream_keys};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub login_limiter: Arc<Mutex<LoginRateLimiter>>,
    pub crypto: Crypto,
    // 票 05/07 起由上游客户端读取
    #[allow(dead_code)]
    pub tavily_base_url: String,
}

pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", auth::router())
        .nest("/api", upstream_keys::router())
        .fallback(assets::static_handler)
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}
