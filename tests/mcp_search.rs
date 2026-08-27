mod common;

use reqwest::StatusCode;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";

/// 无效 / 缺失 / 已吊销的代理密钥都被挡在 /mcp 之外。
#[tokio::test]
async fn mcp_requires_valid_proxy_key() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;
    let token = common::create_proxy_key(&app, "测试客户端").await;

    // 无 key
    let (status, _) = common::mcp_initialize(&app, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 假 key
    let (status, _) = common::mcp_initialize(&app, Some("tp-bogus")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 真 key 可用
    let (status, body) = common::mcp_initialize(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.unwrap()["result"]["serverInfo"]["name"], "tavily-proxy");

    // 吊销后拒绝
    let list: serde_json::Value = app
        .client
        .get(format!("{}/api/proxy-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = list[0]["id"].as_i64().unwrap();
    app.client
        .post(format!("{}/api/proxy-keys/{id}/revoke", app.base_url))
        .send()
        .await
        .unwrap();
    let (status, _) = common::mcp_initialize(&app, Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// tavily_search 端到端：鉴权 → 转发（注入 include_usage）→ 结果返回 → credits 记账。
#[tokio::test]
async fn search_passthrough_end_to_end() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("authorization", format!("Bearer {KEY_A}")))
        .and(body_json(json!({"query": "rust async", "include_usage": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "query": "rust async",
            "results": [{"title": "Async Rust Book", "url": "https://rust-lang.github.io/async-book/", "content": "...", "score": 0.99}],
            "response_time": 0.42,
            "usage": {"credits": 2}
        })))
        .expect(1)
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "主力").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    common::mcp_initialize(&app, Some(&token)).await;

    // tools/list 暴露官方五工具表面（本票只实现 search，其余票陆续补齐）
    let (status, body) = common::mcp_rpc(
        &app,
        Some(&token),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tools = body.unwrap()["result"]["tools"].as_array().unwrap().clone();
    assert!(tools.iter().any(|t| t["name"] == "tavily_search"));

    // tools/call
    let (status, body) = common::mcp_call_tool(
        &app,
        &token,
        3,
        "tavily_search",
        json!({"query": "rust async"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let result = &body["result"];
    assert_ne!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Async Rust Book"), "结果应含上游返回的内容: {text}");

    // credits 记账：上游 key 和代理 key 各记 2
    let keys: serde_json::Value = app
        .client
        .get(format!("{}/api/upstream-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(keys[0]["usage"], 2);
    let keys: serde_json::Value = app
        .client
        .get(format!("{}/api/proxy-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(keys[0]["total_credits"], 2);
}

/// 上游 400 参数错误：以工具错误形式透传上游信息，不算代理自身故障。
#[tokio::test]
async fn upstream_400_is_passed_through_as_tool_error() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "detail": {"error": "Your request is invalid."}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "主力").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    let (_, body) = common::mcp_call_tool(&app, &token, 3, "tavily_search", json!({"query": "x"})).await;
    let result = &body["result"];
    assert_eq!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Your request is invalid."), "应透传上游错误信息: {text}");
}

/// ?key= query 参数传代理密钥（供不能自定义 header 的客户端）。
#[tokio::test]
async fn query_param_auth_works() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;
    let token = common::create_proxy_key(&app, "query 客户端").await;

    let resp = app
        .client
        .post(format!("{}/mcp?key={token}", app.base_url))
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
