//! 选路与失败转移（票 08）：提供商分组随机 + 组内轮询 + 错误驱动的状态机。
//!
//! 分组选路（ADR-0004）：每次工具调用先按提供商（tavily / exa）把健康密钥
//! 分成两组，随机挑一组（有 key 的组等概率），再在该组内做轮询——组内不
//! 再看额度高低，避免某组内额度最好的 key 被打满。
//!
//! 状态机（CONTEXT.md「密钥状态」）：
//! - 429 → 冷却 cooling_until = now + cooldown，到期自动恢复
//! - 432/433（tavily）/ 402（exa）→ 耗尽 exhausted_until = 下一个重置点
//! - 401 → 禁用 + 告警（key_invalid），需人工恢复
//! - 5xx/网络错误 → 不改动状态，本请求内换 key 重试
//! - 其他 4xx（如 400）→ 原样透传，不重试

use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use time::{Date, Month, OffsetDateTime};

use crate::app::AppState;
use crate::auth::now;
use crate::provider::{Kind, Provider, Target, Tool};
use crate::quota;

pub enum Outcome {
    Success {
        payload: Value,
        credits: i64,
        unit: &'static str,
        upstream_key_id: i64,
    },
    /// 上游 4xx（调用方问题），原样带状态码与响应体透传
    Passthrough {
        status: u16,
        payload: Value,
        upstream_key_id: i64,
    },
    /// 全部 key 都不可用
    AllUnavailable,
}

/// 一次执行的入参：工具（search/extract）+ 目标提供商 + 请求体。
pub struct Exec<'a> {
    pub provider: &'a Provider,
    pub tool: Tool,
    pub body: &'a Value,
}

/// 在有健康密钥的提供商组之间等概率随机挑一组（组随机，ADR-0004）。
/// 返回该组 provider；没有任何组有健康 key 时返回 None。
pub async fn pick_group(state: &AppState, _tool: Tool) -> Option<(Kind, &Provider)> {
    sweep_expired(&state.db).await;
    let mut available: Vec<Kind> = Vec::new();
    for provider in state.providers.iter() {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM upstream_keys WHERE status = 'active' AND kind = ?",
        )
        .bind(provider.kind.as_str())
        .fetch_one(&state.db)
        .await
        .ok()?;
        if count > 0 {
            available.push(provider.kind);
        }
    }
    use rand::seq::IndexedRandom;
    let kind = *available.choose(&mut rand::rng())?;
    Some((kind, &state.providers[kind as usize]))
}

/// 带失败转移地执行一次上游调用。
pub async fn execute(state: &AppState, exec: Exec<'_>) -> Outcome {
    let mut tried: HashSet<i64> = HashSet::new();
    loop {
        let Some(target) = pick_next(state, exec.provider, &tried).await else {
            return Outcome::AllUnavailable;
        };
        let path = exec.provider.tool_path(exec.tool);
        match state
            .upstream
            .post_json(
                &exec.provider.base_url,
                exec.provider,
                &target.api_key,
                path,
                exec.body,
            )
            .await
        {
            Ok((status, payload)) if (200..300).contains(&status) => {
                let usage = exec.provider.usage_of(&payload);
                let credits = usage.amount.round() as i64;
                quota::record_usage(&state.db, target.key_id, usage.amount).await;
                return Outcome::Success {
                    payload,
                    credits,
                    unit: usage.unit,
                    upstream_key_id: target.key_id,
                };
            }
            Ok((status, payload)) => match status {
                429 => {
                    mark_cooling(state, target.key_id).await;
                    tried.insert(target.key_id);
                }
                _ if exec.provider.is_exhausted(status) => {
                    mark_exhausted(state, target.key_id).await;
                    tried.insert(target.key_id);
                }
                401 => {
                    mark_invalid(state, target.key_id).await;
                    tried.insert(target.key_id);
                }
                400..=499 => {
                    return Outcome::Passthrough {
                        status,
                        payload,
                        upstream_key_id: target.key_id,
                    };
                }
                _ => {
                    // 5xx：上游抖动，不动状态，换 key 重试
                    tried.insert(target.key_id);
                }
            },
            Err(_) => {
                tried.insert(target.key_id);
            }
        }
    }
}

/// 在指定提供商组内轮询选下一个健康 key：游标按提供商分桶推进，组内均匀轮流。
/// 空组或解密失败返回 None。
async fn pick_next<'a>(
    state: &AppState,
    provider: &'a Provider,
    exclude: &HashSet<i64>,
) -> Option<Target<'a>> {
    sweep_expired(&state.db).await;
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, key_ciphertext FROM upstream_keys \
         WHERE status = 'active' AND kind = ?",
    )
    .bind(provider.kind.as_str())
    .fetch_all(&state.db)
    .await
    .ok()?;
    let candidates: Vec<(i64, String)> = rows
        .into_iter()
        .filter(|(id, _)| !exclude.contains(id))
        .collect();
    if candidates.is_empty() {
        return None;
    }

    let bucket = &state.rr_cursor[provider.kind as usize];
    let start = bucket.fetch_add(1, Ordering::Relaxed) % candidates.len();
    let (key_id, ciphertext) = &candidates[start];
    let api_key = state.crypto.decrypt(ciphertext).ok()?;
    Some(Target::new(provider, *key_id, api_key))
}

/// 把冷却/耗尽到期的 key 恢复为 active，保证 status 列反映当下。
async fn sweep_expired(db: &SqlitePool) {
    let _ = sqlx::query(
        "UPDATE upstream_keys SET status = 'active' \
         WHERE (status = 'cooling' AND cooling_until <= ?) \
            OR (status = 'exhausted' AND exhausted_until <= ?)",
    )
    .bind(now())
    .bind(now())
    .execute(db)
    .await;
}

async fn mark_cooling(state: &AppState, key_id: i64) {
    let until = now() + state.cooldown.as_secs() as i64;
    let _ =
        sqlx::query("UPDATE upstream_keys SET status = 'cooling', cooling_until = ? WHERE id = ?")
            .bind(until)
            .bind(key_id)
            .execute(&state.db)
            .await;
}

async fn mark_exhausted(state: &AppState, key_id: i64) {
    let reset_at = next_reset_at(&state.db, key_id).await;
    let _ = sqlx::query(
        "UPDATE upstream_keys SET status = 'exhausted', exhausted_until = ? WHERE id = ?",
    )
    .bind(reset_at)
    .bind(key_id)
    .execute(&state.db)
    .await;
}

/// 下一个额度重置点：Exa 用本地记账的 quota_reset_at（每月 1 号），Tavily 按 reset_day。
async fn next_reset_at(db: &SqlitePool, key_id: i64) -> i64 {
    let row = sqlx::query_as::<_, (String, Option<i64>, i64)>(
        "SELECT kind, quota_reset_at, reset_day FROM upstream_keys WHERE id = ?",
    )
    .bind(key_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    match row {
        Some((kind, Some(reset_at), _)) if kind == "exa" => reset_at,
        Some((_, _, reset_day)) => next_reset(now(), reset_day),
        None => next_reset(now(), 1),
    }
}

async fn mark_invalid(state: &AppState, key_id: i64) {
    let _ = sqlx::query("UPDATE upstream_keys SET status = 'disabled' WHERE id = ?")
        .bind(key_id)
        .execute(&state.db)
        .await;
    let _ = sqlx::query(
        "INSERT INTO alerts (upstream_key_id, kind, message, created_at) \
         VALUES (?, 'key_invalid', '上游返回 401：密钥无效或已失效，已被自动禁用', ?)",
    )
    .bind(key_id)
    .bind(now())
    .execute(&state.db)
    .await;
}

/// 下一个重置日的 UTC 0 点（reset_day 1-28，任何月份都合法）。
fn next_reset(now_secs: i64, reset_day: i64) -> i64 {
    let now = OffsetDateTime::from_unix_timestamp(now_secs).unwrap();
    let date = now.date();
    let (year, month, day) = (date.year(), date.month(), date.day());
    let target = if (day as i64) < reset_day {
        Date::from_calendar_date(year, month, reset_day as u8).unwrap()
    } else {
        let (next_year, next_month) = match month {
            Month::December => (year + 1, Month::January),
            m => (year, m.next()),
        };
        Date::from_calendar_date(next_year, next_month, reset_day as u8).unwrap()
    };
    target
        .with_hms(0, 0, 0)
        .unwrap()
        .assume_utc()
        .unix_timestamp()
}
