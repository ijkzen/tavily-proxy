mod common;

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";
const KEY_B: &str = "tvly-bbbbbbbbbbbbbbbbbbbb2222";

/// 起一对 key 的测试环境：A 已用 800/1000，B 已用 100/1000。
/// 返回（app, upstream, 代理密钥 token）。
async fn two_key_app(cooldown_secs: u64) -> (common::TestApp, MockServer, String) {
    let upstream = MockServer::start().await;
    for (key, used) in [(KEY_A, 800), (KEY_B, 100)] {
        Mock::given(method("GET"))
            .and(path("/usage"))
            .and(header("authorization", format!("Bearer {key}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "key": {"usage": used, "limit": null},
                "account": {"plan_usage": used, "plan_limit": 1000}
            })))
            .mount(&upstream)
            .await;
    }

    let app = common::spawn_app_tuned(upstream.uri(), 50, cooldown_secs).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    common::add_upstream_key(&app, KEY_B, "B").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    // 等两个 key 的额度都被轮询刷新
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

    (app, upstream, token)
}

/// 让 mock 上游对某个 key 的 /search 返回指定状态。
async fn mock_search(upstream: &MockServer, key: &str, status: u16) {
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("authorization", format!("Bearer {key}")))
        .respond_with(match status {
            200 => ResponseTemplate::new(200).set_body_json(json!({
                "query": "q", "results": [{"title": "ok", "url": "https://example.com", "content": "c", "score": 0.5}],
                "response_time": 0.1, "usage": {"credits": 1}
            })),
            400 => ResponseTemplate::new(400).set_body_json(json!({"detail": {"error": "bad params"}})),
            _ => ResponseTemplate::new(status),
        })
        .mount(upstream)
        .await;
}

/// mock 上游实际收到的 /search 请求所用的 Bearer key 序列。
async fn search_keys_used(upstream: &MockServer) -> Vec<String> {
    upstream
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.url.path() == "/search")
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

async fn upstream_statuses(app: &common::TestApp) -> Vec<(String, String)> {
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
        .map(|k| {
            (
                k["nickname"].as_str().unwrap().to_owned(),
                k["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

async fn call_search(app: &common::TestApp, token: &str) -> serde_json::Value {
    common::mcp_call_tool(app, token, 10, "tavily_search", json!({"query": "q"}))
        .await
        .1
}

/// 组内轮询：同一组内各 key 交替被选中，而不是总选剩余额度最多的。
#[tokio::test]
async fn round_robins_within_group() {
    let (app, upstream, token) = two_key_app(60).await;
    mock_search(&upstream, KEY_A, 200).await;
    mock_search(&upstream, KEY_B, 200).await;

    for _ in 0..4 {
        let body = call_search(&app, &token).await;
        assert_ne!(body["result"]["isError"], true);
    }
    let used = search_keys_used(&upstream).await;
    // 4 次调用应 A/B 交替出现（轮询游标从 0 起：A, B, A, B）
    assert_eq!(
        used,
        vec![KEY_A, KEY_B, KEY_A, KEY_B],
        "组内应轮询: {used:?}"
    );
}

#[tokio::test]
async fn failover_on_429_cools_down_then_recovers() {
    let (app, upstream, token) = two_key_app(1).await; // 冷却 1 秒
    // wiremock 按挂载顺序匹配：先挂一次性 429，耗尽后落到后挂的 200
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("authorization", format!("Bearer {KEY_B}")))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&upstream)
        .await;
    mock_search(&upstream, KEY_B, 200).await;
    mock_search(&upstream, KEY_A, 200).await;

    // 第一次调用：游标落到 A（首个）成功 → A 已用；第二次 B 429 → 转移 A
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.first().unwrap(), KEY_A, "首次应选中 A: {used:?}");
    // 第三次调用前游标已推进，B 先被选中；B 429 → 转移到 A
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.last().unwrap(), KEY_A, "B 冷却后应转移到 A: {used:?}");
    common::eventually(|| async {
        upstream_statuses(&app)
            .await
            .contains(&("B".into(), "cooling".into()))
    })
    .await;

    // 冷却到期后：下一次调用触发 sweep，B 恢复 active，游标轮到 B
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.last().unwrap(), KEY_B, "恢复后应轮到 B: {used:?}");
    assert!(
        upstream_statuses(&app)
            .await
            .contains(&("B".into(), "active".into()))
    );
}

#[tokio::test]
async fn exhausted_on_432_until_reset() {
    let (app, upstream, token) = two_key_app(60).await;
    mock_search(&upstream, KEY_B, 432).await;
    mock_search(&upstream, KEY_A, 200).await;

    // 第一次调用游标落 A → 成功；第二次轮到 B → 432 耗尽 → 转移 A
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.first().unwrap(), KEY_A, "首次应选中 A: {used:?}");
    assert!(
        upstream_statuses(&app)
            .await
            .contains(&("B".into(), "exhausted".into()))
    );

    // 耗尽的 key 不再被选中
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.last().unwrap(), KEY_A, "B 耗尽后只剩 A: {used:?}");
}

#[tokio::test]
async fn disabled_on_401_with_alert() {
    let (app, upstream, token) = two_key_app(60).await;
    mock_search(&upstream, KEY_B, 401).await;
    mock_search(&upstream, KEY_A, 200).await;

    // 第一次调用落 A 成功；第二次轮到 B → 401 禁用 → 转移 A
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.first().unwrap(), KEY_A, "首次应选中 A: {used:?}");
    assert!(
        upstream_statuses(&app)
            .await
            .contains(&("B".into(), "disabled".into()))
    );

    common::eventually(|| async {
        let resp = app
            .client
            .get(format!("{}/api/alerts", app.base_url))
            .send()
            .await
            .unwrap();
        let alerts: serde_json::Value = resp.json().await.unwrap();
        alerts
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "key_invalid")
    })
    .await;
}

#[tokio::test]
async fn failover_on_500() {
    let (app, upstream, token) = two_key_app(60).await;
    mock_search(&upstream, KEY_B, 500).await;
    mock_search(&upstream, KEY_A, 200).await;

    // 首次落 A 成功；第二次轮到 B → 500 抖动 → 转移 A
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let body = call_search(&app, &token).await;
    assert_ne!(body["result"]["isError"], true);
    let used = search_keys_used(&upstream).await;
    assert_eq!(used.first().unwrap(), KEY_A, "首次应选中 A: {used:?}");
    // 5xx 是上游抖动，不改变 key 状态
    assert!(
        upstream_statuses(&app)
            .await
            .contains(&("B".into(), "active".into()))
    );
}

#[tokio::test]
async fn all_unavailable_returns_clear_error() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": {"usage": 0, "limit": null},
            "account": {"plan_usage": 0, "plan_limit": 1000}
        })))
        .mount(&upstream)
        .await;
    mock_search(&upstream, KEY_A, 429).await;

    let app = common::spawn_app_tuned(upstream.uri(), 50, 60).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    let token = common::create_proxy_key(&app, "客户端").await;

    let body = call_search(&app, &token).await;
    assert_eq!(body["result"]["isError"], true);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("不可用"), "应返回明确的不可用错误: {text}");
}
