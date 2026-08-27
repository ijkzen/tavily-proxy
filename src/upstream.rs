//! Tavily REST API 客户端。

use anyhow::Context;
use serde::Deserialize;

#[derive(Clone)]
pub struct UpstreamClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
    pub key: KeyUsage,
    pub account: AccountUsage,
}

#[derive(Debug, Deserialize)]
pub struct KeyUsage {
    pub usage: i64,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AccountUsage {
    pub plan_usage: i64,
    pub plan_limit: Option<i64>,
}

impl UpstreamClient {
    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self { base_url, http }
    }

    /// GET /usage — 额度查询（未文档化端点，ADR-0002）。
    pub async fn fetch_usage(&self, api_key: &str) -> anyhow::Result<UsageResponse> {
        let resp = self
            .http
            .get(format!("{}/usage", self.base_url))
            .bearer_auth(api_key)
            .send()
            .await
            .context("GET /usage 网络错误")?
            .error_for_status()
            .context("GET /usage 非 2xx")?;
        resp.json().await.context("GET /usage 响应解析失败")
    }
}
