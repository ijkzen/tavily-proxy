mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn proxy_keys_require_login() {
    let app = common::spawn_app_no_upstream().await;
    let resp = app
        .client
        .get(format!("{}/api/proxy-keys", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// 签发：完整值只在创建响应里出现一次；列表只显示尾号。
#[tokio::test]
async fn create_shows_full_key_once() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;

    let resp = app
        .client
        .post(format!("{}/api/proxy-keys", app.base_url))
        .json(&serde_json::json!({"name": "笔记本 Claude Code"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let created: serde_json::Value = resp.json().await.unwrap();
    let full_key = created["key"].as_str().unwrap().to_owned();
    assert!(full_key.starts_with("tp-"), "代理密钥应有 tp- 前缀");
    assert_eq!(created["name"], "笔记本 Claude Code");

    let resp = app
        .client
        .get(format!("{}/api/proxy-keys", app.base_url))
        .send()
        .await
        .unwrap();
    let list: serde_json::Value = resp.json().await.unwrap();
    let keys = list.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    let tail = &full_key[full_key.len() - 4..];
    assert_eq!(keys[0]["key_tail"], tail);
    assert_eq!(keys[0]["revoked"], false);
    assert!(
        !serde_json::to_string(&list).unwrap().contains(&full_key),
        "列表不得包含代理密钥明文"
    );
}

/// 吊销立即在列表中可见。（吊销后的 MCP 拒绝在票 07 的测试里覆盖）
#[tokio::test]
async fn revoke_marks_key() {
    let app = common::spawn_app_no_upstream().await;
    common::setup_and_login(&app).await;

    let resp = app
        .client
        .post(format!("{}/api/proxy-keys", app.base_url))
        .json(&serde_json::json!({"name": "临时 key"}))
        .send()
        .await
        .unwrap();
    let id = resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let resp = app
        .client
        .post(format!("{}/api/proxy-keys/{id}/revoke", app.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let list: serde_json::Value = app
        .client
        .get(format!("{}/api/proxy-keys", app.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list[0]["revoked"], true);
}
