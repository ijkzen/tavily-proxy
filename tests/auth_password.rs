mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn change_password_requires_current_and_takes_effect() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;

    // 旧密码不对 → 403
    let resp = app
        .client
        .post(format!("{}/api/password", app.base_url))
        .json(&serde_json::json!({"current_password": "nope", "new_password": "new password 456"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 正确改密
    let resp = app
        .client
        .post(format!("{}/api/password", app.base_url))
        .json(&serde_json::json!({
            "current_password": "correct horse battery staple",
            "new_password": "new password 456"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 改密后旧 session 失效（强制重新登录）
    let resp = app
        .client
        .get(format!("{}/api/me", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 旧密码登录失败，新密码登录成功
    let resp = app
        .client
        .post(format!("{}/api/login", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let resp = app
        .client
        .post(format!("{}/api/login", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "new password 456"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
