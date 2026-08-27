use std::net::TcpListener;
use std::time::Duration;

use tavily_proxy::app::{self, AppState};
use tavily_proxy::db;
use tavily_proxy::upstream::UpstreamClient;

// 部分 helper 字段/方法只被某些测试二进制使用
#[allow(dead_code)]
pub struct TestApp {
    pub base_url: String,
    pub client: reqwest::Client,
    pub tavily_base_url: String,
    _dir: tempfile::TempDir,
}

/// 启动真实 app（临时 SQLite、随机端口），上游 Tavily 指向传入的 base URL
///（测试里传 wiremock server 的地址）。
pub async fn spawn_app(tavily_base_url: String) -> TestApp {
    spawn_app_with(tavily_base_url, Tuning::default()).await
}

/// 额度轮询提速版本：把轮询周期压到毫秒级，供簿记测试驱动。
#[allow(dead_code)]
pub async fn spawn_app_fast_poll(tavily_base_url: String, interval_ms: u64) -> TestApp {
    spawn_app_with(
        tavily_base_url,
        Tuning {
            quota_poll: Duration::from_millis(interval_ms),
            ..Tuning::default()
        },
    )
    .await
}

/// 全参数版本：轮询周期与冷却时长都可调。
#[allow(dead_code)]
pub async fn spawn_app_tuned(
    tavily_base_url: String,
    poll_interval_ms: u64,
    cooldown_secs: u64,
) -> TestApp {
    spawn_app_with(
        tavily_base_url,
        Tuning {
            quota_poll: Duration::from_millis(poll_interval_ms),
            cooldown: Duration::from_secs(cooldown_secs),
            ..Tuning::default()
        },
    )
    .await
}

/// research 编排版本：research 轮询间隔与总超时压到毫秒级；
/// 额度轮询也提速（这类测试需要先把各 key 的额度播进缓存）。
#[allow(dead_code)]
pub async fn spawn_app_research(
    tavily_base_url: String,
    research_poll_ms: u64,
    research_timeout_ms: u64,
) -> TestApp {
    spawn_app_with(
        tavily_base_url,
        Tuning {
            quota_poll: Duration::from_millis(50),
            research_poll: Duration::from_millis(research_poll_ms),
            research_timeout: Duration::from_millis(research_timeout_ms),
            ..Tuning::default()
        },
    )
    .await
}

/// 可调运行参数。默认值与生产配置一致。
#[allow(dead_code)]
pub struct Tuning {
    pub quota_poll: Duration,
    pub cooldown: Duration,
    pub research_poll: Duration,
    pub research_timeout: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            quota_poll: Duration::from_secs(60),
            cooldown: Duration::from_secs(60),
            research_poll: Duration::from_millis(2000),
            research_timeout: Duration::from_secs(600),
        }
    }
}

async fn spawn_app_with(tavily_base_url: String, tuning: Tuning) -> TestApp {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_url = format!("sqlite://{}/test.db", dir.path().display());
    let pool = db::init(&db_url).await.expect("init db");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind port 0");
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).expect("set nonblocking");
    let listener = tokio::net::TcpListener::from_std(listener).expect("to tokio listener");

    let state = AppState {
        crypto: tavily_proxy::crypto::Crypto::load(&pool)
            .await
            .expect("crypto"),
        db: pool,
        login_limiter: Default::default(),
        upstream: UpstreamClient::new(tavily_base_url.clone()),
        quota_poll_interval: tuning.quota_poll,
        cooldown: tuning.cooldown,
        research_timeout: tuning.research_timeout,
        research_poll_interval: tuning.research_poll,
    };
    tokio::spawn(async move {
        axum::serve(listener, app::build(state)).await.unwrap();
    });

    TestApp {
        base_url: format!("http://127.0.0.1:{port}"),
        client: reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("client"),
        tavily_base_url,
        _dir: dir,
    }
}

/// 便捷版本：上游指到一个无人监听的端口（骨架阶段不涉及上游调用）。
#[allow(dead_code)]
pub async fn spawn_app_no_upstream() -> TestApp {
    spawn_app("http://127.0.0.1:9".into()).await
}

/// 创建账号（首访引导），随后的请求都带登录态。
#[allow(dead_code)]
pub async fn setup_and_login(app: &TestApp) {
    let resp = app
        .client
        .post(format!("{}/api/setup", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK, "setup 应成功");
    let resp = app
        .client
        .post(format!("{}/api/login", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK, "login 应成功");
}

/// 轮询断言：每 50ms 重试，最多 3 秒。
#[allow(dead_code)]
pub async fn eventually<F, Fut>(mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..60 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("条件未在 3 秒内满足");
}

// ---------- MCP Streamable HTTP 测试客户端 ----------

/// 发一个 JSON-RPC 请求到 /mcp。proxy_key 同时走 Bearer 头。
/// 返回（HTTP 状态， JSON-RPC 响应体）。兼容 application/json 与 SSE 两种响应。
#[allow(dead_code)]
pub async fn mcp_rpc(
    app: &TestApp,
    proxy_key: Option<&str>,
    body: serde_json::Value,
) -> (reqwest::StatusCode, Option<serde_json::Value>) {
    let mut req = app
        .client
        .post(format!("{}/mcp", app.base_url))
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    if let Some(key) = proxy_key {
        req = req.bearer_auth(key);
    }
    let resp = req.json(&body).send().await.unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let text = resp.text().await.unwrap();

    let parsed = if content_type.contains("text/event-stream") {
        // SSE 帧：取最后一个 data: 载荷
        text.lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .filter_map(|data| serde_json::from_str(data.trim()).ok())
            .last()
    } else {
        serde_json::from_str(&text).ok()
    };
    (status, parsed)
}

/// initialize 握手。
#[allow(dead_code)]
pub async fn mcp_initialize(
    app: &TestApp,
    proxy_key: Option<&str>,
) -> (reqwest::StatusCode, Option<serde_json::Value>) {
    mcp_rpc(
        app,
        proxy_key,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "integration-test", "version": "0.1.0"}
            }
        }),
    )
    .await
}

/// tools/call。
#[allow(dead_code)]
pub async fn mcp_call_tool(
    app: &TestApp,
    proxy_key: &str,
    id: i64,
    tool: &str,
    arguments: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let (status, body) = mcp_rpc(
        app,
        Some(proxy_key),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}
        }),
    )
    .await;
    (status, body.expect("tools/call 应有 JSON-RPC 响应"))
}

/// 签发一个代理密钥，返回完整 token（仅此一次可得）。
#[allow(dead_code)]
pub async fn create_proxy_key(app: &TestApp, name: &str) -> String {
    let resp = app
        .client
        .post(format!("{}/api/proxy-keys", app.base_url))
        .json(&serde_json::json!({"name": name}))
        .send()
        .await
        .unwrap();
    resp.json::<serde_json::Value>().await.unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// 添加一个上游密钥，返回 id。
#[allow(dead_code)]
pub async fn add_upstream_key(app: &TestApp, key: &str, nickname: &str) -> i64 {
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&serde_json::json!({"key": key, "nickname": nickname, "reset_day": 1}))
        .send()
        .await
        .unwrap();
    resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap()
}
