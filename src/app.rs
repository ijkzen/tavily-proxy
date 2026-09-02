use axum::http::StatusCode;
use axum::{Router, routing::get};
use sqlx::SqlitePool;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::auth::LoginRateLimiter;
use crate::crypto::Crypto;
use crate::provider::{Kind, Provider};
use crate::upstream::UpstreamClient;
use crate::{assets, auth, mcp, proxy_keys, quota, request_logs, research, upstream_keys};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub login_limiter: Arc<Mutex<LoginRateLimiter>>,
    pub crypto: Crypto,
    pub upstream: UpstreamClient,
    /// 按 Kind 索引的提供商描述（tavily / exa 各一）。
    pub providers: Arc<Vec<Provider>>,
    /// 组内轮询游标（每提供商一个）。
    pub rr_cursor: Arc<[AtomicUsize; 2]>,
    pub quota_poll_interval: Duration,
    pub cooldown: Duration,
    pub research_timeout: Duration,
    pub research_poll_interval: Duration,
    pub log_retention: Duration,
}

pub fn build(state: AppState) -> Router {
    quota::spawn_poller(state.clone());
    // 上一轮进程里还在轮询的 research 任务标记为中断（账本对齐现实）
    tokio::spawn(research::mark_interrupted_on_boot(state.db.clone()));
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", auth::router())
        .nest("/api", upstream_keys::router())
        .nest("/api", proxy_keys::router())
        .nest("/api", request_logs::router())
        .merge(mcp::router(state.clone()))
        .fallback(assets::static_handler)
        .with_state(state)
}

/// 按 Kind 构造两个提供商（tavily / exa）。
pub fn default_providers(tavily_base_url: String, exa_base_url: String) -> Vec<Provider> {
    vec![
        Provider::new(Kind::Tavily, tavily_base_url),
        Provider::new(Kind::Exa, exa_base_url),
    ]
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}
