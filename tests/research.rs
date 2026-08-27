mod common;

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";
const KEY_B: &str = "tvly-bbbbbbbbbbbbbbbbbbbb2222";

/// 一对 key：A 已用 800/1000，B 已用 100/1000，B 应被选中提交 research。
/// research 轮询 50ms、超时 5 秒。
async fn two_key_app(upstream: &MockServer) -> (common::TestApp, String) {
    for (key, used) in [(KEY_A, 800), (KEY_B, 100)] {
        Mock::given(method("GET"))
            .and(path("/usage"))
            .and(header("authorization", format!("Bearer {key}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "key": {"usage": used, "limit": null},
                "account": {"plan_usage": used, "plan_limit": 1000}
            })))
            .mount(upstream)
            .await;
    }

    let app = common::spawn_app_research(upstream.uri(), 50, 5000).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    common::add_upstream_key(&app, KEY_B, "B").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    common::eventually(|| async {
        let keys: serde_json::Value = app
            .client
            .get(format!("{}/api/upstream-keys", app.base_url))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        keys.as_array().unwrap().len() == 2
            && keys
                .as_array()
                .unwrap()
                .iter()
                .all(|k| k["usage_fetched_at"].is_i64())
    })
    .await;

    (app, token)
}

/// mock 上游 /research 相关请求实际使用的 Bearer key 序列（提交与轮询都算）。
async fn research_keys_used(upstream: &MockServer) -> Vec<(String, String)> {
    upstream
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.url.path().starts_with("/research"))
        .map(|r| {
            let key = r
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .trim_start_matches("Bearer ")
                .to_owned();
            (r.method.to_string(), key)
        })
        .collect()
}

#[tokio::test]
async fn research_waits_for_completion_and_polls_with_same_key() {
    let upstream = MockServer::start().await;
    let (app, token) = two_key_app(&upstream).await;

    // 提交：201 + request_id
    Mock::given(method("POST"))
        .and(path("/research"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "request_id": "r-123", "status": "pending"
        })))
        .mount(&upstream)
        .await;
    // 第一次轮询还在跑（202），第二次完成。wiremock 按挂载顺序匹配。
    Mock::given(method("GET"))
        .and(path("/research/r-123"))
        .respond_with(ResponseTemplate::new(202))
        .up_to_n_times(1)
        .mount(&upstream)
        .await;
    Mock::given(method("GET"))
        .and(path("/research/r-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "request_id": "r-123",
            "status": "completed",
            "content": "调研报告正文",
            "sources": [{"title": "s", "url": "https://example.com"}]
        })))
        .mount(&upstream)
        .await;

    let (_, body) = common::mcp_call_tool(
        &app,
        &token,
        10,
        "tavily_research",
        json!({"input": "调研一下 Rust 异步运行时"}),
    )
    .await;

    assert_ne!(body["result"]["isError"], true, "应成功: {body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("调研报告正文"), "应返回完成载荷: {text}");

    // 提交与全部轮询都落在 B（提交时选中的 key），一次都不能落到 A
    let calls = research_keys_used(&upstream).await;
    assert_eq!(calls.first().unwrap().0, "POST");
    assert!(calls.len() >= 3, "提交 + 至少两次轮询: {calls:?}");
    assert!(
        calls.iter().all(|(_, key)| key == KEY_B),
        "所有 /research 请求都应使用同一 key B: {calls:?}"
    );
}

#[tokio::test]
async fn research_timeout_returns_clear_error() {
    let upstream = MockServer::start().await;
    // 提交成功，但轮询永远 202
    Mock::given(method("POST"))
        .and(path("/research"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "request_id": "r-forever", "status": "pending"
        })))
        .mount(&upstream)
        .await;
    Mock::given(method("GET"))
        .and(path("/research/r-forever"))
        .respond_with(ResponseTemplate::new(202))
        .mount(&upstream)
        .await;

    // 超时压到 300ms
    let app = common::spawn_app_research(upstream.uri(), 50, 300).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    let (_, body) = common::mcp_call_tool(
        &app,
        &token,
        10,
        "tavily_research",
        json!({"input": "永远不会完成的调研"}),
    )
    .await;

    assert_eq!(body["result"]["isError"], true, "超时应报错: {body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("超时"), "错误信息应说明超时: {text}");
}

#[tokio::test]
async fn research_upstream_failed_status_is_surfaced() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/research"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "request_id": "r-fail", "status": "pending"
        })))
        .mount(&upstream)
        .await;
    Mock::given(method("GET"))
        .and(path("/research/r-fail"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "request_id": "r-fail", "status": "failed", "error": "content policy"
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app_research(upstream.uri(), 50, 5000).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    let (_, body) = common::mcp_call_tool(
        &app,
        &token,
        10,
        "tavily_research",
        json!({"input": "会失败的调研"}),
    )
    .await;

    assert_eq!(body["result"]["isError"], true, "上游失败应报错: {body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("失败") && text.contains("content policy"),
        "错误信息应说明上游失败原因: {text}"
    );
}
