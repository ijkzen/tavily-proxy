mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn healthz_returns_200() {
    let app = common::spawn_app_no_upstream().await;
    let resp = app
        .client
        .get(format!("{}/healthz", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn index_page_is_served() {
    let app = common::spawn_app_no_upstream().await;
    let resp = app.client.get(&app.base_url).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert!(body.contains(r#"<div id="root">"#), "应返回前端入口页");
}
