mod common;

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const EXA_KEY: &str = "exa-000000000000000000000000aaaa";

/// Exa 密钥端到端：创建（自动初始化每月 10 美元记账）→ 组内轮询 → costDollars 扣减。
#[tokio::test]
async fn exa_key_round_robin_and_usage_deduction() {
    let upstream = MockServer::start().await;
    // Exa 无 /usage 轮询：GET /usage 不应被调用（仅 Tavily 轮询）
    // 让 mock 对 /search 按 x-api-key 头匹配并返回 costDollars
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("x-api-key", EXA_KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "requestId": "req-1",
            "results": [{"title": "exa result", "url": "https://exa.example.com"}],
            "costDollars": {"total": 0.007}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    common::setup_and_login(&app).await;
    // 显式指定 kind=exa
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&json!({"key": EXA_KEY, "nickname": "Exa主力", "kind": "exa"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "Exa key 创建应成功");
    let created: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(created["kind"], "exa");
    assert_eq!(created["usage"].as_f64().unwrap_or(-1.0), 0.0);
    assert_eq!(
        created["limit"].as_f64().unwrap_or(-1.0),
        10.0,
        "Exa 默认每月 10 美元"
    );

    // 轮询器不应调用 GET /usage（无 Exa 余额接口）——稍后验证 received 里没有
    let token = common::create_proxy_key(&app, "客户端").await;

    // 两次调用 → 组内轮询（单 key 也走轮询，两次都应命中）
    for _ in 0..2 {
        let (_, body) =
            common::mcp_call_tool(&app, &token, 1, "tavily_search", json!({"query": "hello"}))
                .await;
        assert_ne!(body["result"]["isError"], true, "Exa search 应成功: {body}");
        let text = body["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("exa result"), "应返回 Exa 结果: {text}");
    }

    // 上游只应收到带 x-api-key 的 POST /search，绝无 GET /usage
    let requests = upstream.received_requests().await.unwrap_or_default();
    for r in &requests {
        assert_ne!(
            r.method.as_str(),
            "GET",
            "Exa 不应有 GET /usage 轮询: {r:?}"
        );
        assert_eq!(
            r.headers.get("x-api-key").and_then(|v| v.to_str().ok()),
            Some(EXA_KEY),
            "应携带 x-api-key: {r:?}"
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path() == "/search")
            .count(),
        2
    );

    // 本地记账：usage_cached 累加 costDollars（0.007 × 2 = 0.014）
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
        let usage = keys.as_array().unwrap()[0]["usage"]
            .as_f64()
            .unwrap_or(-1.0);
        (usage - 0.014).abs() < 1e-6
    })
    .await;
}

/// Exa 密钥 402（NO_MORE_CREDITS）→ 标记耗尽，直到下月重置点才恢复。
#[tokio::test]
async fn exa_exhausted_on_402() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("x-api-key", EXA_KEY))
        .respond_with(ResponseTemplate::new(402).set_body_json(json!({
            "error": {"code": "NO_MORE_CREDITS", "message": "Account credits are exhausted"}
        })))
        .mount(&upstream)
        .await;

    let app = common::spawn_app(upstream.uri()).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, EXA_KEY, "Exa").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    let (_, body) =
        common::mcp_call_tool(&app, &token, 1, "tavily_search", json!({"query": "q"})).await;
    assert_eq!(body["result"]["isError"], true, "402 应报错: {body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("不可用") || text.contains("上游错误"),
        "应返回明确错误: {text}"
    );

    // 密钥被标记 exhausted（直到下月重置点）
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
        keys.as_array().unwrap()[0]["status"] == "exhausted"
    })
    .await;
}

/// 未显式指定 kind 时按前缀推断：exa- → exa，tvly- → tavily（存量兼容）。
#[tokio::test]
async fn kind_inferred_from_key_prefix() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;

    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&json!({"key": EXA_KEY, "nickname": "Exa隐式"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap()["kind"],
        "exa"
    );

    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&json!({"key": "tvly-zzzzzzzzzzzzzzzzzzzz9999", "nickname": "Tavily隐式"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap()["kind"],
        "tavily"
    );

    // 非法 kind 拒绝
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&json!({"key": EXA_KEY, "nickname": "非法", "kind": "google"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
