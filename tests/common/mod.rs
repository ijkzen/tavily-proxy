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
    spawn_app_with(tavily_base_url, Duration::from_secs(60)).await
}

/// 额度轮询提速版本：把轮询周期压到毫秒级，供簿记测试驱动。
#[allow(dead_code)]
pub async fn spawn_app_fast_poll(tavily_base_url: String, interval_ms: u64) -> TestApp {
    spawn_app_with(tavily_base_url, Duration::from_millis(interval_ms)).await
}

async fn spawn_app_with(tavily_base_url: String, quota_poll_interval: Duration) -> TestApp {
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
        quota_poll_interval,
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
