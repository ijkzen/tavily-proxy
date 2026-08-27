//! tavily_research 同步编排（票 10）：提交 → 同步轮询 → 完成再返回。
//! 任务绑定提交时选中的上游密钥：request_id → upstream_key_id 持久化到
//! research_tasks，轮询始终使用同一把 key，重启后映射不丢。

use serde_json::{Value, json};
use std::time::Instant;

use crate::app::AppState;
use crate::auth::now;
use crate::balancer::{self, Outcome};
use crate::{mcp, quota};

pub enum ResearchOutcome {
    Completed {
        payload: Value,
        credits: i64,
        upstream_key_id: i64,
    },
    /// 提交被上游 4xx 拒绝（调用方问题），透传
    SubmitPassthrough {
        status: u16,
        payload: Value,
        upstream_key_id: i64,
    },
    /// 轮询阶段的失败：超时 / 上游 failed / 任务丢失等
    Failed {
        message: String,
        upstream_key_id: Option<i64>,
    },
    AllUnavailable,
}

pub async fn run(state: &AppState, proxy_key_id: i64, arguments: Value) -> ResearchOutcome {
    let mut body = arguments;
    body["include_usage"] = json!(true);

    let (payload, submit_credits, upstream_key_id) =
        match balancer::execute(state, "/research", &body).await {
            Outcome::Success {
                payload,
                credits,
                upstream_key_id,
            } => (payload, credits, upstream_key_id),
            Outcome::Passthrough {
                status,
                payload,
                upstream_key_id,
            } => {
                return ResearchOutcome::SubmitPassthrough {
                    status,
                    payload,
                    upstream_key_id,
                };
            }
            Outcome::AllUnavailable => return ResearchOutcome::AllUnavailable,
        };
    mcp::record_proxy_usage(state, proxy_key_id, submit_credits).await;

    let Some(request_id) = payload["request_id"].as_str().map(str::to_owned) else {
        return ResearchOutcome::Failed {
            message: "上游未返回 request_id，无法跟踪调研任务".into(),
            upstream_key_id: Some(upstream_key_id),
        };
    };

    let _ = sqlx::query(
        "INSERT INTO research_tasks (request_id, upstream_key_id, proxy_key_id, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&request_id)
    .bind(upstream_key_id)
    .bind(proxy_key_id)
    .bind(now())
    .execute(&state.db)
    .await;

    let Some(api_key) = upstream_api_key(state, upstream_key_id).await else {
        return ResearchOutcome::Failed {
            message: "调研任务绑定的上游密钥已不存在，无法轮询结果".into(),
            upstream_key_id: Some(upstream_key_id),
        };
    };

    poll_until_done(
        state,
        upstream_key_id,
        proxy_key_id,
        submit_credits,
        &request_id,
        &api_key,
    )
    .await
}

/// 服务重启时调用：上一轮进程里还在轮询的任务随请求一起死了，标记为中断，
/// 让 research_tasks 始终是反映当下的事实账本。
pub async fn mark_interrupted_on_boot(db: sqlx::SqlitePool) {
    let _ = sqlx::query(
        "UPDATE research_tasks SET status = 'interrupted', finished_at = ? WHERE status = 'running'",
    )
    .bind(now())
    .execute(&db)
    .await;
}

async fn finish_task(db: &sqlx::SqlitePool, request_id: &str, status: &str) {
    let _ = sqlx::query("UPDATE research_tasks SET status = ?, finished_at = ? WHERE request_id = ?")
        .bind(status)
        .bind(now())
        .bind(request_id)
        .execute(db)
        .await;
}

async fn poll_until_done(
    state: &AppState,
    upstream_key_id: i64,
    proxy_key_id: i64,
    submit_credits: i64,
    request_id: &str,
    api_key: &str,
) -> ResearchOutcome {
    let failed = |message: String| ResearchOutcome::Failed {
        message,
        upstream_key_id: Some(upstream_key_id),
    };
    let deadline = Instant::now() + state.research_timeout;
    let outcome = loop {
        tokio::time::sleep(state.research_poll_interval).await;
        if Instant::now() >= deadline {
            break failed(format!(
                "调研超时：{} 秒内未完成",
                state.research_timeout.as_secs()
            ));
        }
        match state
            .upstream
            .get_json(&format!("/research/{request_id}"), api_key)
            .await
        {
            Ok((200, payload)) => match payload["status"].as_str() {
                Some("completed") => {
                    let credits = payload["usage"]["credits"].as_i64().unwrap_or(0);
                    quota::record_usage(&state.db, upstream_key_id, credits).await;
                    mcp::record_proxy_usage(state, proxy_key_id, credits).await;
                    break ResearchOutcome::Completed {
                        payload,
                        credits: submit_credits + credits,
                        upstream_key_id,
                    };
                }
                Some("failed") => {
                    let reason = mcp::upstream_error_message(&payload);
                    break failed(format!("上游调研任务失败：{reason}"));
                }
                // 200 但仍在处理（pending 等中间态）→ 继续等
                _ => continue,
            },
            // 202：仍在处理
            Ok((202, _)) => continue,
            Ok((404, _)) => {
                break failed("上游找不到该调研任务（可能已过期）".into());
            }
            Ok((status, payload)) => {
                break failed(format!(
                    "轮询调研任务失败（上游 {status}）：{}",
                    mcp::upstream_error_message(&payload)
                ));
            }
            // 网络抖动：不动状态，继续等到超时
            Err(_) => continue,
        }
    };
    let status = match &outcome {
        ResearchOutcome::Completed { .. } => "completed",
        _ => "failed",
    };
    finish_task(&state.db, request_id, status).await;
    outcome
}

async fn upstream_api_key(state: &AppState, upstream_key_id: i64) -> Option<String> {
    let ciphertext: Option<String> =
        sqlx::query_scalar("SELECT key_ciphertext FROM upstream_keys WHERE id = ?")
            .bind(upstream_key_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    ciphertext.and_then(|ct| state.crypto.decrypt(&ct).ok())
}
