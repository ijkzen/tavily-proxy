mod common;

use reqwest::StatusCode;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn add_key(app: &common::TestApp) {
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&json!({"key": "tvly-cccccccccccccccccccc3333", "nickname": "主力", "reset_day": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

async fn first_key(app: &common::TestApp) -> serde_json::Value {
    app.client
        .get(format!("{}/api/upstream-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()[0]
        .clone()
}

/// 轮询 GET /usage 后，上游密钥的用量/额度被刷新（账号粒度：取账号口径）。
#[tokio::test]
async fn poller_refreshes_quota_from_usage_endpoint() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": {"usage": 90, "limit": null},
            "account": {"plan_usage": 100, "plan_limit": 1000}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app_fast_poll(upstream.uri(), 50).await;
    common::setup_and_login(&app).await;
    add_key(&app).await;

    common::eventually(|| async {
        let key = first_key(&app).await;
        key["usage"].as_f64().unwrap_or(-1.0) == 100.0
            && key["limit"].as_f64().unwrap_or(-1.0) == 1000.0
            && key["usage_fetched_at"].is_i64()
    })
    .await;
}

/// 轮询失败：产生告警记录，已有数据保留（本地估算兜底）。
#[tokio::test]
async fn poll_failure_creates_alert() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/usage"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&upstream)
        .await;

    let app = common::spawn_app_fast_poll(upstream.uri(), 50).await;
    common::setup_and_login(&app).await;
    add_key(&app).await;

    common::eventually(|| async {
        let resp = app
            .client
            .get(format!("{}/api/alerts", app.base_url))
            .send()
            .await
            .unwrap();
        let alerts = resp.json::<serde_json::Value>().await.unwrap();
        alerts
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "quota_poll_failed")
    })
    .await;
}
