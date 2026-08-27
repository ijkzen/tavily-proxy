use axum::http::StatusCode;
use axum::{Router, routing::get};
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::auth::LoginRateLimiter;
use crate::crypto::Crypto;
use crate::upstream::UpstreamClient;
use crate::{assets, auth, mcp, proxy_keys, quota, upstream_keys};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub login_limiter: Arc<Mutex<LoginRateLimiter>>,
    pub crypto: Crypto,
    pub upstream: UpstreamClient,
    pub quota_poll_interval: Duration,
}

pub fn build(state: AppState) -> Router {
    quota::spawn_poller(state.clone());
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", auth::router())
        .nest("/api", upstream_keys::router())
        .nest("/api", proxy_keys::router())
        .merge(mcp::router(state.clone()))
        .fallback(assets::static_handler)
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}
