mod common;

use reqwest::StatusCode;

/// 连续登录失败触发限速：锁定后即使密码正确也返回 429。
#[tokio::test]
async fn repeated_login_failures_are_rate_limited() {
    let app = common::spawn_app_no_upstream().await;
    app.client
        .post(format!("{}/api/setup", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();

    for _ in 0..5 {
        let resp = app
            .client
            .post(format!("{}/api/login", app.base_url))
            .json(&serde_json::json!({"username": "zen", "password": "wrong password"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 第 6 次：已被限速
    let resp = app
        .client
        .post(format!("{}/api/login", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "wrong password"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // 正确密码也被挡（锁定未到期）
    let resp = app
        .client
        .post(format!("{}/api/login", app.base_url))
        .json(&serde_json::json!({"username": "zen", "password": "correct horse battery staple"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}
