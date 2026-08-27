#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub tavily_base_url: String,
    pub quota_poll_interval_secs: u64,
    pub cooldown_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data/tavily-proxy.db".into()),
            tavily_base_url: std::env::var("TAVILY_BASE_URL")
                .unwrap_or_else(|_| "https://api.tavily.com".into()),
            quota_poll_interval_secs: std::env::var("QUOTA_POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            cooldown_secs: std::env::var("COOLDOWN_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        }
    }
}
