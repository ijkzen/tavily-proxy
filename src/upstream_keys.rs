use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, Router, routing::get, routing::post};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::auth::{AuthUser, now};
use crate::provider::Kind;
use crate::quota;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/upstream-keys", get(list).post(create))
        .route("/upstream-keys/{id}/disable", post(disable))
        .route("/upstream-keys/{id}/enable", post(enable))
        .route("/upstream-keys/{id}/reveal", post(reveal))
        .route("/upstream-keys/{id}", axum::routing::delete(remove))
        .route("/alerts", get(alerts))
}

/// 最近的告警（如额度轮询失败、key 失效），供看板展示。
async fn alerts(State(state): State<AppState>, _user: AuthUser) -> Result<Json<Value>, StatusCode> {
    let rows = sqlx::query_as::<_, (i64, Option<i64>, String, String, i64)>(
        "SELECT id, upstream_key_id, kind, message, created_at \
         FROM alerts ORDER BY id DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(|(id, key_id, kind, message, created_at)| {
                json!({
                    "id": id,
                    "upstream_key_id": key_id,
                    "kind": kind,
                    "message": message,
                    "created_at": created_at,
                })
            })
            .collect(),
    )))
}

/// 列表/详情里给前端的上游密钥视图——绝不含明文。
/// usage 用 f64（CAST）：tavily 存整数 credits，exa 存美元浮点（本地记账）。
type KeyRow = (
    i64,
    String,
    String,
    String,
    String,
    i64,
    f64,
    Option<f64>,
    Option<i64>,
    i64,
);

fn to_json(row: KeyRow) -> Value {
    let (id, nickname, tail, status, kind, reset_day, usage, limit, fetched_at, created_at) = row;
    json!({
        "id": id,
        "nickname": nickname,
        "key_tail": tail,
        "status": status,
        "kind": kind,
        "reset_day": reset_day,
        "usage": usage,
        "limit": limit,
        "usage_fetched_at": fetched_at,
        "created_at": created_at,
    })
}

const LIST_SQL: &str = "SELECT id, nickname, key_tail, status, kind, reset_day, \
     CAST(usage_cached AS REAL), CAST(limit_cached AS REAL), usage_fetched_at, created_at \
     FROM upstream_keys ORDER BY id";

async fn list(State(state): State<AppState>, _user: AuthUser) -> Result<Json<Value>, StatusCode> {
    let rows = sqlx::query_as::<_, KeyRow>(LIST_SQL)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Value::Array(rows.into_iter().map(to_json).collect())))
}

#[derive(Deserialize)]
struct CreateRequest {
    key: String,
    nickname: String,
    kind: Option<String>,
    reset_day: Option<i64>,
}

async fn create(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(body): Json<CreateRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let key = body.key.trim();
    let nickname = body.nickname.trim();
    let reset_day = body.reset_day.unwrap_or(1);
    // 类型显式指定（tavily / exa）；未传时按前缀推断（存量调用方兼容）
    let kind = match body.kind.as_deref() {
        Some("tavily") => Kind::Tavily,
        Some("exa") => Kind::Exa,
        Some(_) => return Err(StatusCode::BAD_REQUEST),
        None => Kind::from_key(key),
    };
    if key.is_empty() || nickname.is_empty() || !(1..=28).contains(&reset_day) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let ciphertext = state
        .crypto
        .encrypt(key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    let result = sqlx::query(
        "INSERT INTO upstream_keys (nickname, key_ciphertext, key_tail, kind, reset_day, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(nickname)
    .bind(ciphertext)
    .bind(&tail)
    .bind(kind.as_str())
    .bind(reset_day)
    .bind(now())
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Exa 密钥初始化本地记账：每月 10 美元，重置点为下月 1 号（ADR-0004）
    if kind == Kind::Exa {
        quota::init_exa_quota(&state.db, result.last_insert_rowid()).await;
    }

    let row = sqlx::query_as::<_, KeyRow>(&format!("{LIST_SQL} /* created */"))
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|(id, ..)| *id == result.last_insert_rowid())
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::OK, Json(to_json(row))))
}

async fn set_status(
    state: &AppState,
    id: i64,
    from_any: bool,
    status: &str,
) -> Result<(), StatusCode> {
    let sql = if from_any {
        "UPDATE upstream_keys SET status = ? WHERE id = ?"
    } else {
        "UPDATE upstream_keys SET status = ? WHERE id = ? AND status = 'disabled'"
    };
    let result = sqlx::query(sql)
        .bind(status)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(())
}

async fn disable(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    set_status(&state, id, true, "disabled").await?;
    Ok(StatusCode::OK)
}

async fn enable(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    // 只允许从「禁用」恢复；冷却/耗尽由状态机自己管理
    set_status(&state, id, false, "active").await?;
    Ok(StatusCode::OK)
}

/// 返回完整明文（密钥本就由用户自己添加，加密存储仅为落库安全）。
async fn reveal(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    let ciphertext =
        sqlx::query_scalar::<_, String>("SELECT key_ciphertext FROM upstream_keys WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
    let key = state
        .crypto
        .decrypt(&ciphertext)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "key": key })))
}

async fn remove(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM upstream_keys WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::OK)
}
