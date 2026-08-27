mod common;

use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";
const KEY_B: &str = "tvly-bbbbbbbbbbbbbbbbbbbb2222";

/// 三个同步工具各跑一次端到端透传。
#[tokio::test]
async fn extract_crawl_map_passthrough() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/extract"))
        .and(body_json(json!({"urls": ["https://example.com"], "include_usage": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"url": "https://example.com", "raw_content": "extracted"}],
            "failed_results": [], "response_time": 0.2, "usage": {"credits": 1}
        })))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/crawl"))
        .and(body_json(json!({"url": "https://example.com", "include_usage": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base_url": "https://example.com",
            "results": [{"url": "https://example.com", "raw_content": "crawled"}],
            "response_time": 1.0, "usage": {"credits": 3}
        })))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/map"))
        .and(body_json(json!({"url": "https://example.com", "include_usage": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base_url": "https://example.com",
            "results": ["https://example.com/a", "https://example.com/b"],
            "response_time": 0.8, "usage": {"credits": 1}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    // tools/list 现在暴露四个工具
    let (_, body) = common::mcp_rpc(&app, Some(&token), json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await;
    let tools = body.unwrap()["result"]["tools"].as_array().unwrap().clone();
    for name in ["tavily_search", "tavily_extract", "tavily_crawl", "tavily_map"] {
        assert!(tools.iter().any(|t| t["name"] == name), "缺少工具 {name}");
    }

    for (id, tool, args, marker) in [
        (10, "tavily_extract", json!({"urls": ["https://example.com"]}), "extracted"),
        (11, "tavily_crawl", json!({"url": "https://example.com"}), "crawled"),
        (12, "tavily_map", json!({"url": "https://example.com"}), "example.com/b"),
    ] {
        let (_, body) = common::mcp_call_tool(&app, &token, id, tool, args).await;
        let result = &body["result"];
        assert_ne!(result["isError"], true, "{tool} 不应报错: {result}");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains(marker), "{tool} 结果应包含 {marker}: {text}");
    }
}

/// 三个同步工具复用同一条选路/状态机管道：crawl 遇 500 换 key。
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
    // B 剩余更多 → 首选 B；B 的 /crawl 500，A 的 200
    Mock::given(method("POST"))
        .and(path("/crawl"))
        .and(header("authorization", format!("Bearer {KEY_B}")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/crawl"))
        .and(header("authorization", format!("Bearer {KEY_A}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base_url": "https://example.com", "results": [], "response_time": 1.0, "usage": {"credits": 1}
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
        keys.as_array().unwrap().iter().all(|k| k["usage_fetched_at"].is_i64())
    })
    .await;

    let (_, body) = common::mcp_call_tool(&app, &token, 10, "tavily_crawl", json!({"url": "https://example.com"})).await;
    assert_ne!(body["result"]["isError"], true);

    let crawls: Vec<String> = upstream
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.url.path() == "/crawl")
        .map(|r| {
            r.headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .trim_start_matches("Bearer ")
                .to_owned()
        })
        .collect();
    assert_eq!(crawls, vec![KEY_B, KEY_A], "应先试 B，500 后转移 A");
}
