//! 代理密钥：本应用签发给调用方的 tp- 密钥（见 CONTEXT.md）。

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, Router, routing::get, routing::post};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::app::AppState;
use crate::auth::{AuthUser, now, sha256_hex};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proxy-keys", get(list).post(create))
        .route("/proxy-keys/{id}/revoke", post(revoke))
        .route("/proxy-keys/{id}/reveal", post(reveal))
}

type ProxyKeyRow = (i64, String, String, i64, Option<i64>, i64);

const LIST_SQL: &str = "SELECT id, name, key_tail, total_credits, last_used_at, \
     created_at FROM proxy_keys ORDER BY id";

fn to_json(row: ProxyKeyRow) -> Value {
    let (id, name, tail, total_credits, last_used_at, created_at) = row;
    json!({
        "id": id,
        "name": name,
        "key_tail": tail,
        "total_credits": total_credits,
        "last_used_at": last_used_at,
        "created_at": created_at,
    })
}

async fn list(State(state): State<AppState>, _user: AuthUser) -> Result<Json<Value>, StatusCode> {
    let rows = sqlx::query_as::<_, ProxyKeyRow>(LIST_SQL)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Value::Array(rows.into_iter().map(to_json).collect())))
}

#[derive(Deserialize)]
struct CreateRequest {
    name: String,
}

async fn create(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(body): Json<CreateRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut raw = [0u8; 24];
    rand::rng().fill_bytes(&mut raw);
    let token = format!("tp-{}", hex::encode(raw));
    let tail = token[token.len() - 4..].to_owned();
    // 双写：哈希用于验证，AES-GCM 密文用于事后明文可见/复制
    let ciphertext = state
        .crypto
        .encrypt(&token)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query(
        "INSERT INTO proxy_keys (name, key_hash, key_tail, key_ciphertext, created_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(name)
    .bind(sha256_hex(&token))
    .bind(&tail)
    .bind(ciphertext)
    .bind(now())
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut view = to_json(
        sqlx::query_as::<_, ProxyKeyRow>("SELECT id, name, key_tail, total_credits, last_used_at, created_at FROM proxy_keys WHERE id = ?")
            .bind(result.last_insert_rowid())
            .fetch_one(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    // 完整值仅此一次返回
    view["key"] = Value::String(token);
    Ok((StatusCode::OK, Json(view)))
}

async fn revoke(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    // 吊销即删除：代理密钥本就只服务我们自己签发的调用方，无审计必要；
    // request_logs.proxy_key_id 变悬空引用，列表 JOIN 侧显示为「已删除密钥」。
    let result = sqlx::query("DELETE FROM proxy_keys WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::OK)
}

/// 返回完整明文（双写改造前创建的旧 key 没有密文，返回 409）。
async fn reveal(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    let row = sqlx::query_scalar::<_, Option<String>>(
        "SELECT key_ciphertext FROM proxy_keys WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ciphertext = match row {
        None => return Err(StatusCode::NOT_FOUND),
        // 旧 key：无明文可示，需吊销重建
        Some(None) => return Err(StatusCode::CONFLICT),
        Some(Some(ct)) => ct,
    };
    let key = state
        .crypto
        .decrypt(&ciphertext)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "key": key })))
}

/// 校验代理密钥（MCP 端点鉴权用）：有效则返回 id 并刷新最近使用时间。
pub async fn verify(db: &SqlitePool, token: &str) -> Option<i64> {
    let id = sqlx::query_scalar::<_, i64>("SELECT id FROM proxy_keys WHERE key_hash = ?")
        .bind(sha256_hex(token))
        .fetch_optional(db)
        .await
        .ok()??;
    let _ = sqlx::query("UPDATE proxy_keys SET last_used_at = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(db)
        .await;
    Some(id)
}
