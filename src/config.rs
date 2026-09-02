#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub tavily_base_url: String,
    pub exa_base_url: String,
    pub quota_poll_interval_secs: u64,
    pub cooldown_secs: u64,
    pub research_timeout_secs: u64,
    pub research_poll_interval_ms: u64,
    pub log_retention_days: u64,
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
            exa_base_url: std::env::var("EXA_BASE_URL")
                .unwrap_or_else(|_| "https://api.exa.ai".into()),
            quota_poll_interval_secs: std::env::var("QUOTA_POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            cooldown_secs: std::env::var("COOLDOWN_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            research_timeout_secs: std::env::var("RESEARCH_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(600),
            research_poll_interval_ms: std::env::var("RESEARCH_POLL_INTERVAL_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2000),
            log_retention_days: std::env::var("LOG_RETENTION_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        }
    }
}
