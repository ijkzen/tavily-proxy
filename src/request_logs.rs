//! 请求日志与统计（票 11）：每次 MCP 工具调用落 request_logs，保留 30 天。
//! 看板 API：GET /api/logs（筛选+分页）、GET /api/stats（成功率与延迟聚合）。

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::app::AppState;
use crate::auth::{AuthUser, now};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/logs", get(list_logs))
        .route("/stats", get(stats))
}

// ---------- 写入 ----------

pub struct NewLog {
    pub proxy_key_id: i64,
    pub tool: String,
    pub params_summary: String,
    pub upstream_key_id: Option<i64>,
    pub credits: i64,
    pub duration_ms: i64,
    pub success: bool,
    pub error: Option<String>,
}

pub async fn record(db: &SqlitePool, log: NewLog) {
    let _ = sqlx::query(
        "INSERT INTO request_logs \
         (proxy_key_id, tool, params_summary, upstream_key_id, credits, duration_ms, success, error, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(log.proxy_key_id)
    .bind(log.tool)
    .bind(log.params_summary)
    .bind(log.upstream_key_id)
    .bind(log.credits)
    .bind(log.duration_ms)
    .bind(log.success)
    .bind(log.error)
    .bind(now())
    .execute(db)
    .await;
}

/// 清理超过保留期的日志（额度轮询器每个周期顺带调用）。
pub async fn cleanup_expired(db: &SqlitePool, retention: std::time::Duration) {
    let _ = sqlx::query("DELETE FROM request_logs WHERE created_at < ?")
        .bind(now() - retention.as_secs() as i64)
        .execute(db)
        .await;
}

/// 参数摘要：截断到 500 字符，避免日志表被大参数撑爆。
pub fn summarize_params(arguments: &Value) -> String {
    let s = arguments.to_string();
    if s.chars().count() > 500 {
        s.chars().take(500).collect::<String>() + "…"
    } else {
        s
    }
}

// ---------- 查询 ----------

#[derive(Deserialize)]
struct LogsQuery {
    proxy_key_id: Option<i64>,
    tool: Option<String>,
    success: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct LogRow {
    id: i64,
    proxy_key_id: Option<i64>,
    proxy_key_name: Option<String>,
    tool: String,
    params_summary: Option<String>,
    upstream_key_id: Option<i64>,
    upstream_key_nickname: Option<String>,
    credits: i64,
    duration_ms: i64,
    success: i64,
    error: Option<String>,
    created_at: i64,
}

fn log_to_json(row: LogRow) -> Value {
    json!({
        "id": row.id,
        "proxy_key_id": row.proxy_key_id,
        "proxy_key_name": row.proxy_key_name,
        "tool": row.tool,
        "params_summary": row.params_summary,
        "upstream_key_id": row.upstream_key_id,
        "upstream_key_nickname": row.upstream_key_nickname,
        "credits": row.credits,
        "duration_ms": row.duration_ms,
        "success": row.success != 0,
        "error": row.error,
        "created_at": row.created_at,
    })
}

async fn list_logs(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    let mut cond = String::from("WHERE 1=1");
    if q.proxy_key_id.is_some() {
        cond.push_str(" AND l.proxy_key_id = ?");
    }
    if q.tool.is_some() {
        cond.push_str(" AND l.tool = ?");
    }
    if q.success.is_some() {
        cond.push_str(" AND l.success = ?");
    }

    let count_sql = format!("SELECT COUNT(*) FROM request_logs l {cond}");
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(id) = q.proxy_key_id {
        count_q = count_q.bind(id);
    }
    if let Some(tool) = &q.tool {
        count_q = count_q.bind(tool.clone());
    }
    if let Some(success) = q.success {
        count_q = count_q.bind(success);
    }
    let total: i64 = count_q
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items_sql = format!(
        "SELECT l.id, l.proxy_key_id, pk.name AS proxy_key_name, l.tool, l.params_summary, \
         l.upstream_key_id, uk.nickname AS upstream_key_nickname, l.credits, l.duration_ms, \
         l.success, l.error, l.created_at \
         FROM request_logs l \
         LEFT JOIN proxy_keys pk ON pk.id = l.proxy_key_id \
         LEFT JOIN upstream_keys uk ON uk.id = l.upstream_key_id \
         {cond} ORDER BY l.id DESC LIMIT ? OFFSET ?"
    );
    let mut items_q = sqlx::query_as::<_, LogRow>(&items_sql);
    if let Some(id) = q.proxy_key_id {
        items_q = items_q.bind(id);
    }
    if let Some(tool) = &q.tool {
        items_q = items_q.bind(tool.clone());
    }
    if let Some(success) = q.success {
        items_q = items_q.bind(success);
    }
    let rows = items_q
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "total": total,
        "items": rows.into_iter().map(log_to_json).collect::<Vec<_>>(),
    })))
}

/// 保留期窗口内（默认近 30 天）的聚合：总量、成功数/率、平均与 p95 延迟、总 credits。
async fn stats(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<Value>, StatusCode> {
    let cutoff = now() - state.log_retention.as_secs() as i64;
    let rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT success, duration_ms FROM request_logs WHERE created_at >= ?",
    )
    .bind(cutoff)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_credits: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(credits), 0) FROM request_logs WHERE created_at >= ?",
    )
    .bind(cutoff)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total = rows.len() as i64;
    let successes = rows.iter().filter(|(s, _)| *s != 0).count() as i64;
    let success_rate = if total > 0 {
        successes as f64 / total as f64
    } else {
        0.0
    };
    let avg_duration = if total > 0 {
        rows.iter().map(|(_, d)| d).sum::<i64>() as f64 / total as f64
    } else {
        0.0
    };
    let mut durations: Vec<i64> = rows.iter().map(|(_, d)| *d).collect();
    durations.sort_unstable();
    let p95 = if durations.is_empty() {
        0
    } else {
        durations[((durations.len() as f64 - 1.0) * 0.95).ceil() as usize]
    };

    Ok(Json(json!({
        "total": total,
        "successes": successes,
        "success_rate": success_rate,
        "avg_duration_ms": avg_duration,
        "p95_duration_ms": p95,
        "total_credits": total_credits,
    })))
}
