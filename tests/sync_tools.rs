mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";
const KEY_B: &str = "tvly-bbbbbbbbbbbbbbbbbbbb2222";

/// extract 端到端透传；tools/list 只暴露 search/extract 两个工具。
#[tokio::test]
async fn extract_passthrough_and_tools_list() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/extract"))
        .and(body_json(
            json!({"urls": ["https://example.com"], "include_usage": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"url": "https://example.com", "raw_content": "extracted"}],
            "failed_results": [], "response_time": 0.2, "usage": {"credits": 1}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    // tools/list 只暴露 search 和 extract
    let (_, body) = common::mcp_rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let tools = body.unwrap()["result"]["tools"].as_array().unwrap().clone();
    assert_eq!(tools.len(), 2, "应只暴露两个工具: {tools:?}");
    for name in ["tavily_search", "tavily_extract"] {
        assert!(tools.iter().any(|t| t["name"] == name), "缺少工具 {name}");
    }
    for hidden in ["tavily_crawl", "tavily_map", "tavily_research"] {
        assert!(
            !tools.iter().any(|t| t["name"] == hidden),
            "不应暴露已隐藏工具 {hidden}: {tools:?}"
        );
    }

    // extract 透传
    let (_, body) = common::mcp_call_tool(
        &app,
        &token,
        10,
        "tavily_extract",
        json!({"urls": ["https://example.com"]}),
    )
    .await;
    let result = &body["result"];
    assert_ne!(result["isError"], true, "tavily_extract 不应报错: {result}");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("extracted"),
        "extract 结果应包含 extracted: {text}"
    );

    // 已隐藏工具调用应返回未知工具错误
    for hidden in ["tavily_crawl", "tavily_map", "tavily_research"] {
        let (_, body) = common::mcp_call_tool(
            &app,
            &token,
            11,
            hidden,
            json!({"url": "https://example.com"}),
        )
        .await;
        assert_eq!(body["result"]["isError"], true, "{hidden} 应被拒绝: {body}");
    }
}

/// 同步工具复用同一条选路/状态机管道：extract 遇 500 换 key。
#[tokio::test]
async fn sync_tools_share_failover_pipeline() {
    let upstream = MockServer::start().await;
    for key in [KEY_A, KEY_B] {
        Mock::given(method("GET"))
            .and(path("/usage"))
            .and(header("authorization", format!("Bearer {key}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "key": {"usage": if key == KEY_A { 900 } else { 100 }, "limit": null},
                "account": {"plan_usage": if key == KEY_A { 900 } else { 100 }, "plan_limit": 1000}
            })))
            .mount(&upstream)
            .await;
    }
    // 组内轮询：第一次调用落 A（成功）；第二次轮到 B，B 的 /extract 500 → 转移 A
    Mock::given(method("POST"))
        .and(path("/extract"))
        .and(header("authorization", format!("Bearer {KEY_B}")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/extract"))
        .and(header("authorization", format!("Bearer {KEY_A}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"url": "https://example.com", "raw_content": "ok"}],
            "failed_results": [], "response_time": 1.0, "usage": {"credits": 1}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app_fast_poll(upstream.uri(), 50).await;
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
        keys.as_array()
            .unwrap()
            .iter()
            .all(|k| k["usage_fetched_at"].is_i64())
    })
    .await;

    // 第一次调用：游标落 A → 成功
    let (_, body) = common::mcp_call_tool(
        &app,
        &token,
        10,
        "tavily_extract",
        json!({"urls": ["https://example.com"]}),
    )
    .await;
    assert_ne!(body["result"]["isError"], true);
    let extracts = extract_keys_used(&upstream).await;
    assert_eq!(extracts, vec![KEY_A], "首次应选中 A: {extracts:?}");

    // 第二次调用：轮到 B → 500 → 转移 A 成功（失败转移生效）
    let (_, body) = common::mcp_call_tool(
        &app,
        &token,
        11,
        "tavily_extract",
        json!({"urls": ["https://example.com"]}),
    )
    .await;
    assert_ne!(body["result"]["isError"], true);
    let extracts = extract_keys_used(&upstream).await;
    assert_eq!(
        extracts,
        vec![KEY_A, KEY_B, KEY_A],
        "B 500 后应转移 A: {extracts:?}"
    );
}

/// mock 上游实际收到的 /extract 请求所用的 Bearer key 序列。
async fn extract_keys_used(upstream: &MockServer) -> Vec<String> {
    upstream
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.url.path() == "/extract")
        .map(|r| {
            r.headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .trim_start_matches("Bearer ")
                .to_owned()
        })
        .collect()
}
