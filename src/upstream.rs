//! Tavily REST API 客户端。

use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;

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

    /// POST 一个 JSON 到上游端点（/search 等），返回（状态码，解析后的响应体）。
    pub async fn post_json(
        &self,
        path: &str,
        api_key: &str,
        body: &Value,
    ) -> anyhow::Result<(u16, Value)> {
        let resp = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(api_key)
            .json(body)
            .send()
            .await
            .context("POST 上游网络错误")?;
        let status = resp.status().as_u16();
        let text = resp.text().await.context("读取上游响应失败")?;
        let json = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok((status, json))
    }
}
