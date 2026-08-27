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

use std::time::Instant;

use crate::app::AppState;
use crate::{balancer, proxy_keys, request_logs, research};

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
            call_tool(&state, proxy_key_id, id, &name, arguments).await        }
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
    let bool_default_false = json!({"type": "boolean", "default": false});
    let string_array = json!({"type": "array", "items": {"type": "string"}});
    let extract_depth = json!({"type": "string", "enum": ["basic", "advanced"], "default": "basic"});
    let format_enum = json!({"type": "string", "enum": ["markdown", "text"], "default": "markdown"});

    vec![
        json!({
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
                    "include_domains": string_array,
                    "exclude_domains": string_array,
                    "country": {"type": "string"},
                    "include_favicon": {"type": "boolean", "default": false},
                    "exact_match": {"type": "boolean", "default": false}
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "tavily_extract",
            "description": "Extract raw content from one or more URLs (markdown or text).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "urls": {"type": "array", "items": {"type": "string"}, "description": "URLs to extract (max 20)"},
                    "extract_depth": extract_depth,
                    "include_images": bool_default_false,
                    "include_favicon": {"type": "boolean", "default": false},
                    "format": format_enum,
                    "query": {"type": "string", "description": "Focus extraction on content relevant to this query"}
                },
                "required": ["urls"]
            }
        }),
        json!({
            "name": "tavily_crawl",
            "description": "Crawl a website starting from a URL, extracting content from pages with configurable depth and breadth.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The root URL to begin the crawl"},
                    "max_depth": {"type": "integer", "minimum": 1, "default": 1},
                    "max_breadth": {"type": "integer", "minimum": 1, "default": 20},
                    "limit": {"type": "integer", "minimum": 1, "default": 50},
                    "instructions": {"type": "string", "description": "Natural language instructions for the crawler"},
                    "select_paths": string_array,
                    "select_domains": string_array,
                    "allow_external": {"type": "boolean", "default": true},
                    "extract_depth": extract_depth,
                    "format": format_enum,
                    "include_favicon": {"type": "boolean", "default": false}
                },
                "required": ["url"]
            }
        }),
        json!({
            "name": "tavily_map",
            "description": "Map a website's structure. Returns a list of URLs found starting from the base URL.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The root URL to begin the mapping"},
                    "max_depth": {"type": "integer", "minimum": 1, "default": 1},
                    "max_breadth": {"type": "integer", "minimum": 1, "default": 20},
                    "limit": {"type": "integer", "minimum": 1, "default": 50},
                    "instructions": {"type": "string"},
                    "select_paths": string_array,
                    "select_domains": string_array,
                    "allow_external": {"type": "boolean", "default": true}
                },
                "required": ["url"]
            }
        }),
        json!({
            "name": "tavily_research",
            "description": "Performs comprehensive research on a given topic using multiple sources. Returns a detailed report. This is a long-running operation that waits for completion.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": {"type": "string", "description": "The research task or question"},
                    "model": {"type": "string", "enum": ["mini", "pro", "auto"], "default": "auto"},
                    "instructions": {"type": "string", "description": "Custom instructions for the research agent"},
                    "output_schema": {"type": "object", "description": "JSON schema the report should conform to"}
                },
                "required": ["input"]
            }
        }),
    ]
}

async fn call_tool(
    state: &AppState,
    proxy_key_id: i64,
    id: Value,
    name: &str,
    arguments: Value,
) -> Response {
    if name == "tavily_research" {
        return call_research_tool(state, proxy_key_id, id, arguments).await;
    }
    // 同步工具：名称 → 上游 REST 路径
    let path = match name {
        "tavily_search" => "/search",
        "tavily_extract" => "/extract",
        "tavily_crawl" => "/crawl",
        "tavily_map" => "/map",
        _ => return tool_error(id, format!("未知工具: {name}")),
    };
    call_sync_tool(state, proxy_key_id, id, name, path, arguments).await
}

/// tavily_research：提交 + 同步轮询编排（research.rs）。
async fn call_research_tool(
    state: &AppState,
    proxy_key_id: i64,
    id: Value,
    arguments: Value,
) -> Response {
    let started = Instant::now();
    let params_summary = request_logs::summarize_params(&arguments);
    let outcome = research::run(state, proxy_key_id, arguments).await;
    let duration_ms = started.elapsed().as_millis() as i64;

    let (upstream_key_id, success, error) = match &outcome {
        research::ResearchOutcome::Completed { upstream_key_id, .. } => {
            (Some(*upstream_key_id), true, None)
        }
        research::ResearchOutcome::SubmitPassthrough {
            status,
            payload,
            upstream_key_id,
        } => (
            Some(*upstream_key_id),
            false,
            Some(format!("上游错误 {status}: {}", upstream_error_message(payload))),
        ),
        research::ResearchOutcome::Failed { message, upstream_key_id } => {
            (*upstream_key_id, false, Some(message.clone()))
        }
        research::ResearchOutcome::AllUnavailable => (
            None,
            false,
            Some("所有上游密钥暂不可用（限流/耗尽/已禁用）".to_owned()),
        ),
    };
    let credits = match &outcome {
        research::ResearchOutcome::Completed { credits, .. } => *credits,
        _ => 0,
    };
    request_logs::record(
        &state.db,
        request_logs::NewLog {
            proxy_key_id,
            tool: "tavily_research".to_owned(),
            params_summary,
            upstream_key_id,
            credits,
            duration_ms,
            success,
            error,
        },
    )
    .await;

    match outcome {
        research::ResearchOutcome::Completed { payload, .. } => tool_success(id, &payload),
        research::ResearchOutcome::SubmitPassthrough { status, payload, .. } => {
            tool_error(id, format!("上游错误 {status}: {}", upstream_error_message(&payload)))
        }
        research::ResearchOutcome::Failed { message, .. } => tool_error(id, message),
        research::ResearchOutcome::AllUnavailable => {
            tool_error(id, "所有上游密钥暂不可用（限流/耗尽/已禁用），请稍后重试或检查密钥池")
        }
    }
}

/// 同步工具通用管道：注入 include_usage → 选路器+状态机 → 记账/透传 → 落日志。
async fn call_sync_tool(
    state: &AppState,
    proxy_key_id: i64,
    id: Value,
    tool: &str,
    path: &str,
    arguments: Value,
) -> Response {
    let started = Instant::now();
    let params_summary = request_logs::summarize_params(&arguments);
    let mut body = arguments;
    body["include_usage"] = json!(true);

    let outcome = balancer::execute(state, path, &body).await;
    let duration_ms = started.elapsed().as_millis() as i64;
    let (upstream_key_id, credits, success, error) = match &outcome {
        balancer::Outcome::Success {
            credits,
            upstream_key_id,
            ..
        } => (Some(*upstream_key_id), *credits, true, None),
        balancer::Outcome::Passthrough {
            status,
            payload,
            upstream_key_id,
        } => (
            Some(*upstream_key_id),
            0,
            false,
            Some(format!("上游错误 {status}: {}", upstream_error_message(payload))),
        ),
        balancer::Outcome::AllUnavailable => (
            None,
            0,
            false,
            Some("所有上游密钥暂不可用（限流/耗尽/已禁用）".to_owned()),
        ),
    };
    request_logs::record(
        &state.db,
        request_logs::NewLog {
            proxy_key_id,
            tool: tool.to_owned(),
            params_summary,
            upstream_key_id,
            credits,
            duration_ms,
            success,
            error,
        },
    )
    .await;

    match outcome {
        balancer::Outcome::Success { payload, credits, .. } => {
            record_proxy_usage(state, proxy_key_id, credits).await;
            tool_success(id, &payload)
        }
        balancer::Outcome::Passthrough { status, payload, .. } => {
            tool_error(id, format!("上游错误 {status}: {}", upstream_error_message(&payload)))
        }
        balancer::Outcome::AllUnavailable => {
            tool_error(id, "所有上游密钥暂不可用（限流/耗尽/已禁用），请稍后重试或检查密钥池")
        }
    }
}

pub(crate) async fn record_proxy_usage(state: &AppState, proxy_key_id: i64, credits: i64) {
    let _ = sqlx::query("UPDATE proxy_keys SET total_credits = total_credits + ? WHERE id = ?")
        .bind(credits)
        .bind(proxy_key_id)
        .execute(&state.db)
        .await;
}

/// 从 Tavily 错误体里提取可读信息：{"detail": {"error": "…"}} 或 {"detail": "…"}。
pub(crate) fn upstream_error_message(payload: &Value) -> String {
    payload
        .pointer("/detail/error")
        .and_then(Value::as_str)
        .or_else(|| payload["detail"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| payload.to_string())
}
