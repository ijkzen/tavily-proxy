mod common;

use reqwest::StatusCode;

async fn add_key(app: &common::TestApp, key: &str, nickname: &str) -> i64 {
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys", app.base_url))
        .json(&serde_json::json!({"key": key, "nickname": nickname, "reset_day": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap()
}

async fn list(app: &common::TestApp) -> Vec<serde_json::Value> {
    app.client
        .get(format!("{}/api/upstream-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn disable_enable_delete() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;
    let id = add_key(&app, "tvly-bbbbbbbbbbbbbbbbbbbb2222", "备用").await;

    // 禁用
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys/{id}/disable", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(list(&app).await[0]["status"], "disabled");

    // 启用（从禁用恢复）
    let resp = app
        .client
        .post(format!("{}/api/upstream-keys/{id}/enable", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(list(&app).await[0]["status"], "active");

    // 删除
    let resp = app
        .client
        .delete(format!("{}/api/upstream-keys/{id}", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(list(&app).await.is_empty());

    // 删除不存在的 id → 404
    let resp = app
        .client
        .delete(format!("{}/api/upstream-keys/{id}", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
