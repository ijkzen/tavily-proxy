use axum::http::StatusCode;
use axum::{Router, routing::get};
use sqlx::SqlitePool;

use crate::assets;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    // 票 05/07 起由上游客户端读取
    #[allow(dead_code)]
    pub tavily_base_url: String,
}

pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .fallback(assets::static_handler)
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}
