mod common;

use reqwest::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 测试基建自检：mock 上游 server 可以作为 tavily_base_url 注入 app。
#[tokio::test]
async fn mock_upstream_is_pluggable() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "query": "stub", "results": [], "response_time": 0.01
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    // app 已带着 mock 上游地址启动；此处直接验证 mock server 自身按预期工作，
    // 后续票的工具调用会经由 app 打到它。
    let resp = app
        .client
        .post(format!("{}/search", app.tavily_base_url))
        .json(&serde_json::json!({"query": "stub"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(app.tavily_base_url, upstream.uri());
}
