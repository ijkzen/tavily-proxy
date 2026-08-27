mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn login_me_logout() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;

    // 登录成功后可访问 /api/me
    let resp = app
        .client
        .get(format!("{}/api/me", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["username"], "zen");

    // 登出后 session 失效
    let resp = app
        .client
        .post(format!("{}/api/logout", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .client
        .get(format!("{}/api/me", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_password_is_rejected() {
    let app = common::spawn_app_no_upstream().await;
    app.client
        .post(format!("{}/api/setup", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();

    let resp = app
        .client
        .post(format!("{}/api/login", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "wrong password"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = app
        .client
        .get(format!("{}/api/me", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
