mod common;

use reqwest::StatusCode;

/// 首访引导：无用户时开放；创建账号后关闭，再次创建被拒绝。
#[tokio::test]
async fn setup_flow_closes_after_first_account() {
    let app = common::spawn_app_no_upstream().await;

    let resp = app
        .client
        .get(format!("{}/api/setup/status", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["needs_setup"], true);

    let resp = app
        .client
        .post(format!("{}/api/setup", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = app
        .client
        .get(format!("{}/api/setup/status", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["needs_setup"], false);

    // 引导已关闭：再次创建必须被拒绝
    let resp = app
        .client
        .post(format!("{}/api/setup", app.base_url))
        .json(&serde_json::json!({"username": "eve", "password": "eve password 123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
