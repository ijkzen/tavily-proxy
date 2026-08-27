mod common;

use serde_json::json;

const UPSTREAM_KEY: &str = "tvly-reveal-test-key-1234567890";

#[tokio::test]
async fn upstream_key_plaintext_can_be_revealed() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;
    let id = common::add_upstream_key(&app, UPSTREAM_KEY, "主力").await;

    let resp = app
        .client
        .post(format!("{}/api/upstream-keys/{id}/reveal", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["key"], UPSTREAM_KEY, "应返回完整明文");
}

#[tokio::test]
async fn proxy_key_plaintext_can_be_revealed_after_creation() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;
    let token = common::create_proxy_key(&app, "笔记本").await;

    // 从列表找到刚创建的 key id
    let keys: serde_json::Value = app
        .client
        .get(format!("{}/api/proxy-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = keys.as_array().unwrap()[0]["id"].as_i64().unwrap();

    let resp = app
        .client
        .post(format!("{}/api/proxy-keys/{id}/reveal", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["key"], json!(token), "revealed 明文应与签发时一致");
}

#[tokio::test]
async fn reveal_requires_login() {
    let app = common::spawn_app_no_upstream().await;
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys/1/reveal", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}
