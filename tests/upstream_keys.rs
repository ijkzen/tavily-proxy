mod common;

use reqwest::StatusCode;

const KEY_A: &str = "tvly-aaaaaaaaaaaaaaaaaaaa1111";

#[tokio::test]
async fn upstream_keys_require_login() {
    let app = common::spawn_app_no_upstream().await;
    let resp = app
        .client
        .get(format!("{}/api/upstream-keys", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn add_and_list_shows_tail_only() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;

    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&serde_json::json!({
            "key": KEY_A,
            "nickname": "主力 key",
            "reset_day": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .client
        .get(format!("{}/api/upstream-keys", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let keys = body.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["nickname"], "主力 key");
    assert_eq!(keys[0]["key_tail"], "1111");
    assert_eq!(keys[0]["status"], "active");

    // 明文密钥绝不出现在接口响应里
    let raw = serde_json::to_string(&body).unwrap();
    assert!(!raw.contains(KEY_A), "响应不得包含上游密钥明文");
}
