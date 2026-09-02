mod common;

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";
const KEY_B: &str = "tvly-bbbbbbbbbbbbbbbbbbbb2222";

/// 清理逻辑直测：超过保留期（30 天）的日志整行删除（含 params_json / response_json），
/// 未到期的保留。这是「完整响应会吃空间」的兜底——到期后大响应一并清掉。
#[tokio::test]
async fn cleanup_expired_deletes_old_rows_with_details() {
    let pool = common::new_db().await;
    let now = common::now();
    let insert = |created_at: i64| {
        sqlx::query(
            "INSERT INTO request_logs \
             (proxy_key_id, tool, params_summary, params_json, response_json, upstream_key_id, \
              credits, duration_ms, success, error, created_at) \
             VALUES (NULL, 'tavily_extract', 's', '{\"urls\":[]}', \
                     '{\"results\":[{\"text\":\"big markdown\"}]}', NULL, 1, 100, 1, NULL, ?)",
        )
        .bind(created_at)
    };
    insert(now - 31 * 24 * 3600).execute(&pool).await.unwrap();
    insert(now - 24 * 3600).execute(&pool).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "应插入两条日志");

    tavily_proxy::request_logs::cleanup_expired(
        &pool,
        std::time::Duration::from_secs(30 * 24 * 3600),
    )
    .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "31 天前的行（含大响应）应被删除");
    let created_at: i64 = sqlx::query_scalar("SELECT created_at FROM request_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(created_at > now - 30 * 24 * 3600, "未到期的行应保留");
}

/// 一对 key：A 已用 800/1000，B 已用 100/1000，B 应被选中。
/// 返回（app, upstream, key_b_id）。
async fn two_key_app(upstream: &MockServer) -> (common::TestApp, i64) {
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
    let app = common::spawn_app_tuned(upstream.uri(), 50, 60).await;
    common::setup_and_login(&app).await;
    common::add_upstream_key(&app, KEY_A, "A").await;
    let key_b_id = common::add_upstream_key(&app, KEY_B, "B").await;
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
    (app, key_b_id)
}

async fn get_logs(app: &common::TestApp, query: &str) -> serde_json::Value {
    app.client
        .get(format!("{}/api/logs{query}", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn get_stats(app: &common::TestApp) -> serde_json::Value {
    app.client
        .get(format!("{}/api/stats", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn requests_are_logged_with_filters_and_stats() {
    let upstream = MockServer::start().await;
    let (app, key_b_id) = two_key_app(&upstream).await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "query": "q", "results": [], "usage": {"credits": 2}
        })))
        .mount(&upstream)
        .await;

    let token1 = common::create_proxy_key(&app, "客户端一").await;
    let token2 = common::create_proxy_key(&app, "客户端二").await;
    let proxy_key1_id: i64 = {
        let keys: serde_json::Value = app
            .client
            .get(format!("{}/api/proxy-keys", app.base_url))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        keys.as_array().unwrap()[0]["id"].as_i64().unwrap()
    };

    common::mcp_call_tool(
        &app,
        &token1,
        1,
        "tavily_search",
        json!({"query": "第一个查询"}),
    )
    .await;
    common::mcp_call_tool(
        &app,
        &token2,
        2,
        "tavily_search",
        json!({"query": "第二个查询"}),
    )
    .await;

    // 全部日志：两条，含完整字段
    let logs = get_logs(&app, "").await;
    assert_eq!(logs["total"], 2, "应有两条日志: {logs}");
    let items = logs["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    // 新的在前
    let first = &items[0];
    assert_eq!(first["tool"], "tavily_search");
    assert_eq!(first["success"], true);
    assert_eq!(first["credits"], 2);
    assert!(first["duration_ms"].is_i64(), "应记录耗时: {first}");
    assert_eq!(first["upstream_key_id"], key_b_id, "应记录实际所用上游密钥");
    assert!(first["proxy_key_id"].is_i64());
    assert!(
        first["params_summary"]
            .as_str()
            .unwrap()
            .contains("第二个查询"),
        "应记录参数摘要: {first}"
    );
    // 明细：完整参数与响应体
    let params: serde_json::Value =
        serde_json::from_str(first["params_json"].as_str().unwrap()).unwrap();
    assert_eq!(params["query"], "第二个查询", "应存完整入参: {first}");
    let resp: serde_json::Value =
        serde_json::from_str(first["response_json"].as_str().unwrap()).unwrap();
    assert_eq!(resp["usage"]["credits"], 2, "应存完整响应体: {first}");
    assert_eq!(first["upstream_key_kind"], "tavily");
    assert!(first["error"].is_null());

    // 按上游密钥提供商筛选：全是 tavily
    let logs = get_logs(&app, "?kind=tavily").await;
    assert_eq!(logs["total"], 2);
    let logs = get_logs(&app, "?kind=exa").await;
    assert_eq!(logs["total"], 0);

    // 按代理密钥筛选
    let logs = get_logs(&app, &format!("?proxy_key_id={proxy_key1_id}")).await;
    assert_eq!(logs["total"], 1);
    assert_eq!(logs["items"][0]["proxy_key_id"], proxy_key1_id);
    assert!(
        logs["items"][0]["params_summary"]
            .as_str()
            .unwrap()
            .contains("第一个查询")
    );

    // 按工具筛选：没有 extract 调用
    let logs = get_logs(&app, "?tool=tavily_extract").await;
    assert_eq!(logs["total"], 0);
    let logs = get_logs(&app, "?tool=tavily_search").await;
    assert_eq!(logs["total"], 2);

    // 按成败筛选：都成功
    let logs = get_logs(&app, "?success=true").await;
    assert_eq!(logs["total"], 2);
    let logs = get_logs(&app, "?success=false").await;
    assert_eq!(logs["total"], 0);

    // 聚合统计
    let stats = get_stats(&app).await;
    assert_eq!(stats["total"], 2);
    assert_eq!(stats["successes"], 2);
    assert_eq!(stats["success_rate"], 1.0);
    assert!(stats["avg_duration_ms"].is_f64() || stats["avg_duration_ms"].is_i64());
    assert!(stats["p95_duration_ms"].is_i64());
    assert_eq!(stats["total_credits"], 4);
}

#[tokio::test]
async fn failed_requests_are_logged_and_counted() {
    let upstream = MockServer::start().await;
    let (app, key_b_id) = two_key_app(&upstream).await;
    // 一次成功 + 一次 400 透传（调用方参数问题）
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "query": "q", "results": [], "usage": {"credits": 1}
        })))
        .up_to_n_times(1)
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "detail": {"error": "bad params"}
        })))
        .mount(&upstream)
        .await;

    let token = common::create_proxy_key(&app, "客户端").await;
    common::mcp_call_tool(&app, &token, 1, "tavily_search", json!({"query": "好的"})).await;
    let (_, bad) =
        common::mcp_call_tool(&app, &token, 2, "tavily_search", json!({"query": "坏的"})).await;
    assert_eq!(bad["result"]["isError"], true);

    let logs = get_logs(&app, "?success=false").await;
    assert_eq!(logs["total"], 1, "失败日志应可筛出: {logs}");
    let entry = &logs["items"][0];
    assert_eq!(entry["success"], false);
    assert_eq!(entry["credits"], 0);
    assert_eq!(
        entry["upstream_key_id"], key_b_id,
        "透传失败也应记下当时所用 key"
    );
    let error = entry["error"].as_str().unwrap();
    assert!(error.contains("400"), "错误信息应含上游状态码: {error}");

    let stats = get_stats(&app).await;
    assert_eq!(stats["total"], 2);
    assert_eq!(stats["successes"], 1);
    assert_eq!(stats["success_rate"], 0.5);
    assert_eq!(stats["total_credits"], 1);
}
