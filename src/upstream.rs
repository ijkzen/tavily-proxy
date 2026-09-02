//! 上游 REST 客户端：向指定基址发起带鉴权的 JSON 请求。
//! 基址与鉴权方式由调用方（provider.rs 的 Provider）决定。

use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone)]
pub struct UpstreamClient {
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
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self { http }
    }

    /// GET /usage — 额度查询（未文档化端点，ADR-0002）。仅 Tavily 支持。
    pub async fn fetch_usage(
        &self,
        base_url: &str,
        api_key: &str,
    ) -> anyhow::Result<UsageResponse> {
        let resp = self
            .http
            .get(format!("{base_url}/usage"))
            .bearer_auth(api_key)
            .send()
            .await
            .context("GET /usage 网络错误")?
            .error_for_status()
            .context("GET /usage 非 2xx")?;
        resp.json().await.context("GET /usage 响应解析失败")
    }

    /// POST 一个 JSON 到上游端点（/search 等），返回（状态码，解析后的响应体）。
    /// 鉴权头由 provider 决定（tavily Bearer / exa x-api-key）。
    pub async fn post_json(
        &self,
        base_url: &str,
        provider: &crate::provider::Provider,
        api_key: &str,
        path: &str,
        body: &Value,
    ) -> anyhow::Result<(u16, Value)> {
        let req = self.http.post(format!("{base_url}{path}")).json(body);
        let Some(req) = provider.auth(req, api_key) else {
            return Err(anyhow::anyhow!("密钥无法通过 {} 鉴权", provider.name));
        };
        self.send(req).await
    }

    /// GET 一个上游端点（/research/{id} 轮询等），返回（状态码，解析后的响应体）。
    pub async fn get_json(
        &self,
        base_url: &str,
        api_key: &str,
        path: &str,
    ) -> anyhow::Result<(u16, Value)> {
        let req = self
            .http
            .get(format!("{base_url}{path}"))
            .bearer_auth(api_key);
        self.send(req).await
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> anyhow::Result<(u16, Value)> {
        let resp = req.send().await.context("上游网络错误")?;
        let status = resp.status().as_u16();
        let text = resp.text().await.context("读取上游响应失败")?;
        let json = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok((status, json))
    }
}

impl Default for UpstreamClient {
    fn default() -> Self {
        Self::new()
    }
}
