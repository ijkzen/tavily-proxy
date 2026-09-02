//! MCP Streamable HTTP 端点（ADR-0001）：自实现的 JSON-RPC over HTTP 表面。
//! 对外只暴露 `tavily_search` 与 `tavily_extract` 两个工具；crawl/map/research 的上游转发
//! 与轮询编排（balancer.rs / research.rs）保留，但不向 MCP 客户端暴露。
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
use crate::provider::{Kind, Tool};
use crate::{balancer, proxy_keys, request_logs};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/mcp",
            post(handle_post).get(handle_get).delete(handle_delete),
        )
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
    let bool_default_false = json!({"type": "boolean", "default": false});
    let string_array = json!({"type": "array", "items": {"type": "string"}});
    let extract_depth =
        json!({"type": "string", "enum": ["basic", "advanced"], "default": "basic"});
    let format_enum =
        json!({"type": "string", "enum": ["markdown", "text"], "default": "markdown"});

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
    ]
}

async fn call_tool(
    state: &AppState,
    proxy_key_id: i64,
    id: Value,
    name: &str,
    arguments: Value,
) -> Response {
    // 同步工具：工具名 → 语义工具；tavily_search/extract 在 tavily 与 exa
    // 两组之间随机选组（组内轮询），见 ADR-0004
    let tool_kind = match name {
        "tavily_search" => Tool::Search,
        "tavily_extract" => Tool::Extract,
        _ => return tool_error(id, format!("未知工具: {name}")),
    };
    call_sync_tool(state, proxy_key_id, id, name, tool_kind, arguments).await
}

/// 同步工具通用管道：组随机 → 参数翻译 → 选路器+状态机 → 记账/透传 → 落日志。
async fn call_sync_tool(
    state: &AppState,
    proxy_key_id: i64,
    id: Value,
    tool: &str,
    tool_kind: Tool,
    arguments: Value,
) -> Response {
    let log = ToolLog::start(proxy_key_id, tool, &arguments);

    // 组随机：在有健康 key 的提供商组中等概率挑一组；一组都没有 → 不可用
    let Some((provider_kind, provider)) = balancer::pick_group(state, tool_kind).await else {
        let msg = "所有上游密钥暂不可用（限流/耗尽/已禁用），请稍后重试或检查密钥池";
        log.finish(state, None, 0, false, None, Some(msg.to_owned()))
            .await;
        return tool_error(id, msg);
    };

    // 按选中组的原生参数翻译；tavily 额外注入 include_usage
    let mut body = match provider.translate(tool_kind, &arguments) {
        Some(b) => b,
        None => {
            let msg = "参数对所选提供商无效";
            log.finish(state, None, 0, false, None, Some(msg.to_owned()))
                .await;
            return tool_error(id, msg);
        }
    };
    if provider_kind == Kind::Tavily {
        body["include_usage"] = json!(true);
    }

    match balancer::execute(
        state,
        balancer::Exec {
            provider,
            tool: tool_kind,
            body: &body,
        },
    )
    .await
    {
        balancer::Outcome::Success {
            payload,
            credits,
            unit: _,
            upstream_key_id,
        } => {
            log.finish(
                state,
                Some(upstream_key_id),
                credits,
                true,
                Some(&payload),
                None,
            )
            .await;
            record_proxy_usage(state, proxy_key_id, credits).await;
            tool_success(id, &payload)
        }
        balancer::Outcome::Passthrough {
            status,
            payload,
            upstream_key_id,
        } => {
            let msg = format!("上游错误 {status}: {}", upstream_error_message(&payload));
            log.finish(
                state,
                Some(upstream_key_id),
                0,
                false,
                Some(&payload),
                Some(msg.clone()),
            )
            .await;
            tool_error(id, msg)
        }
        balancer::Outcome::AllUnavailable => {
            let msg = "所有上游密钥暂不可用（限流/耗尽/已禁用），请稍后重试或检查密钥池";
            log.finish(state, None, 0, false, None, Some(msg.to_owned()))
                .await;
            tool_error(id, msg)
        }
    }
}

/// 工具调用日志上下文：调用开始时 start，结束时 finish 落账（两个工具管道共用）。
struct ToolLog {
    proxy_key_id: i64,
    tool: String,
    /// 完整入参 JSON 文本（明细弹窗展示用）。
    params_json: String,
    params_summary: String,
    started: Instant,
}

impl ToolLog {
    fn start(proxy_key_id: i64, tool: &str, arguments: &Value) -> Self {
        Self {
            proxy_key_id,
            tool: tool.to_owned(),
            params_json: arguments.to_string(),
            params_summary: request_logs::summarize_params(arguments),
            started: Instant::now(),
        }
    }

    /// success 时带响应体落账（response_json）；失败时 response 为 None。
    async fn finish(
        self,
        state: &AppState,
        upstream_key_id: Option<i64>,
        credits: i64,
        success: bool,
        response: Option<&Value>,
        error: Option<String>,
    ) {
        request_logs::record(
            &state.db,
            request_logs::NewLog {
                proxy_key_id: self.proxy_key_id,
                tool: self.tool,
                params_summary: self.params_summary,
                params_json: self.params_json,
                response_json: response.map(Value::to_string),
                upstream_key_id,
                credits,
                duration_ms: self.started.elapsed().as_millis() as i64,
                success,
                error,
            },
        )
        .await;
    }
}

pub(crate) async fn record_proxy_usage(state: &AppState, proxy_key_id: i64, credits: i64) {
    let _ = sqlx::query("UPDATE proxy_keys SET total_credits = total_credits + ? WHERE id = ?")
        .bind(credits)
        .bind(proxy_key_id)
        .execute(&state.db)
        .await;
}

/// 从上游错误体里提取可读信息：{"detail": {"error": "…"}} / {"detail": "…"} / {"error": "…"}。
pub(crate) fn upstream_error_message(payload: &Value) -> String {
    payload
        .pointer("/detail/error")
        .and_then(Value::as_str)
        .or_else(|| payload["detail"].as_str())
        .or_else(|| payload["error"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| payload.to_string())
}
