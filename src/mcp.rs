//! MCP Streamable HTTP 端点（ADR-0001）：自实现的 JSON-RPC over HTTP 表面。
//! 无状态：不签发 Mcp-Session-Id，响应一律 application/json（spec 允许的非流式形态）。
//! 鉴权：代理密钥，`Authorization: Bearer tp-…` 或 `?key=tp-…`。

use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, middleware, routing::post};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::{balancer, proxy_keys, quota};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/mcp", post(handle_post).get(handle_get).delete(handle_delete))
        .route_layer(middleware::from_fn_with_state(state, auth_middleware))
}

// ---------- 鉴权 ----------

/// 通过鉴权后注入 request extensions 的代理密钥 id。
#[derive(Clone, Copy)]
pub struct ProxyKeyId(pub i64);

#[derive(Deserialize)]
struct KeyQuery {
    key: Option<String>,
}

async fn auth_middleware(
    State(state): State<AppState>,
    Query(query): Query<KeyQuery>,
    mut request: Request,
    next: Next,
) -> Response {
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .map(str::to_owned);
    let token = bearer.or(query.key);
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return unauthorized();
    };
    match proxy_keys::verify(&state.db, &token).await {
        Some(id) => {
            request.extensions_mut().insert(ProxyKeyId(id));
            next.run(request).await
        }
        None => unauthorized(),
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "无效或已吊销的代理密钥"})),
    )
        .into_response()
}

// ---------- JSON-RPC ----------

async fn handle_get() -> StatusCode {
    // 不支持独立的 SSE 监听流（spec 允许返回 405）
    StatusCode::METHOD_NOT_ALLOWED
}

async fn handle_delete() -> StatusCode {
    StatusCode::OK
}

async fn handle_post(
    State(state): State<AppState>,
    axum::Extension(ProxyKeyId(proxy_key_id)): axum::Extension<ProxyKeyId>,
    Json(body): Json<Value>,
) -> Response {
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");

    match method {
        "initialize" => rpc_result(
            id,
            json!({
                "protocolVersion": body["params"]["protocolVersion"].as_str().unwrap_or("2025-03-26"),
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "tavily-proxy", "version": env!("CARGO_PKG_VERSION")}
            }),
        ),
        "ping" => rpc_result(id, json!({})),
        m if m.starts_with("notifications/") => StatusCode::ACCEPTED.into_response(),
        "tools/list" => rpc_result(id, json!({"tools": tools()})),
        "tools/call" => {
            let name = body["params"]["name"].as_str().unwrap_or("").to_owned();
            let arguments = body["params"]["arguments"].clone();
            call_tool(&state, proxy_key_id, id, &name, arguments).await
        }
        _ => (
            StatusCode::OK,
            Json(json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": format!("未知方法: {method}")}
            })),
        )
            .into_response(),
    }
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
}

fn tool_success(id: Value, payload: &Value) -> Response {
    rpc_result(
        id,
        json!({
            "content": [{"type": "text", "text": payload.to_string()}],
            "structuredContent": payload,
            "isError": false
        }),
    )
}

fn tool_error(id: Value, message: impl Into<String>) -> Response {
    rpc_result(
        id,
        json!({
            "content": [{"type": "text", "text": message.into()}],
            "isError": true
        }),
    )
}

// ---------- 工具 ----------

fn tools() -> Vec<Value> {
    vec![json!({
        "name": "tavily_search",
        "description": "A search engine optimized for comprehensive, accurate, and trusted results. Useful for when you need to answer questions about current events. Input should be a search query.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query"},
                "max_results": {"type": "integer", "description": "Maximum number of results (0-20)", "default": 5},
                "search_depth": {"type": "string", "enum": ["basic", "advanced", "fast", "ultra-fast"], "default": "basic"},
                "topic": {"type": "string", "enum": ["general", "news", "finance"], "default": "general"},
                "time_range": {"type": "string", "enum": ["day", "week", "month", "year"]},
                "start_date": {"type": "string", "description": "YYYY-MM-DD"},
                "end_date": {"type": "string", "description": "YYYY-MM-DD"},
                "include_answer": {"type": "boolean", "default": false},
                "include_images": {"type": "boolean", "default": false},
                "include_image_descriptions": {"type": "boolean", "default": false},
                "include_raw_content": {"type": ["boolean", "string"], "default": false},
                "include_domains": {"type": "array", "items": {"type": "string"}},
                "exclude_domains": {"type": "array", "items": {"type": "string"}},
                "country": {"type": "string"},
                "include_favicon": {"type": "boolean", "default": false},
                "exact_match": {"type": "boolean", "default": false}
            },
            "required": ["query"]
        }
    })]
}

async fn call_tool(
    state: &AppState,
    proxy_key_id: i64,
    id: Value,
    name: &str,
    arguments: Value,
) -> Response {
    match name {
        "tavily_search" => call_search(state, proxy_key_id, id, arguments).await,
        _ => tool_error(id, format!("未知工具: {name}")),
    }
}

/// 票 07：单 key 直转（额度感知选路与失败转移在票 08）。
async fn call_search(state: &AppState, proxy_key_id: i64, id: Value, arguments: Value) -> Response {
    let Some((key_id, ciphertext)) = balancer::pick_any_active(&state.db).await else {
        return tool_error(id, "没有可用的上游密钥，请先在管理界面添加");
    };
    let api_key = match state.crypto.decrypt(&ciphertext) {
        Ok(k) => k,
        Err(_) => return tool_error(id, "上游密钥解密失败"),
    };

    let mut upstream_body = arguments;
    upstream_body["include_usage"] = json!(true);

    match state.upstream.post_json("/search", &api_key, &upstream_body).await {
        Ok((200, payload)) => {
            let credits = payload["usage"]["credits"].as_i64().unwrap_or(0);
            quota::record_usage(&state.db, key_id, credits).await;
            record_proxy_usage(&state, proxy_key_id, credits).await;
            tool_success(id, &payload)
        }
        Ok((status, payload)) => {
            tool_error(id, format!("上游错误 {status}: {}", upstream_error_message(&payload)))
        }
        Err(err) => tool_error(id, format!("请求上游失败: {err:#}")),
    }
}

async fn record_proxy_usage(state: &AppState, proxy_key_id: i64, credits: i64) {
    let _ = sqlx::query("UPDATE proxy_keys SET total_credits = total_credits + ? WHERE id = ?")
        .bind(credits)
        .bind(proxy_key_id)
        .execute(&state.db)
        .await;
}

/// 从 Tavily 错误体里提取可读信息：{"detail": {"error": "…"}} 或 {"detail": "…"}。
fn upstream_error_message(payload: &Value) -> String {
    payload
        .pointer("/detail/error")
        .and_then(Value::as_str)
        .or_else(|| payload["detail"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| payload.to_string())
}
