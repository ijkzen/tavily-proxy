use anyhow::Context;
use tavily_proxy::app::{self, AppState};
use tavily_proxy::config::Config;
use tavily_proxy::db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tavily_proxy=info".into()),
        )
        .init();

    let config = Config::from_env();
    if let Some(parent) = config
        .database_url
        .strip_prefix("sqlite://")
        .and_then(|p| std::path::Path::new(p).parent())
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).context("create database directory")?;
    }

    let pool = db::init(&config.database_url).await?;
    let crypto = tavily_proxy::crypto::Crypto::load(&pool).await?;
    let state = AppState {
        db: pool,
        login_limiter: Default::default(),
        crypto,
        upstream: tavily_proxy::upstream::UpstreamClient::new(config.tavily_base_url.clone()),
        quota_poll_interval: std::time::Duration::from_secs(config.quota_poll_interval_secs),
    };

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(port = config.port, "listening");
    axum::serve(listener, app::build(state)).await?;
    Ok(())
}
