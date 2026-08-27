//! 选路与失败转移（票 08）：额度感知选路 + 错误驱动的状态机。
//!
//! 状态机（CONTEXT.md「密钥状态」）：
//! - 429 → 冷却 cooling_until = now + cooldown，到期自动恢复
//! - 432/433 → 耗尽 exhausted_until = 下一个重置日 0 点（UTC）
//! - 401 → 禁用 + 告警（key_invalid），需人工恢复
//! - 5xx/网络错误 → 不改动状态，本请求内换 key 重试
//! - 其他 4xx（如 400）→ 原样透传，不重试

use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashSet;
use time::{Date, Month, OffsetDateTime};

use crate::app::AppState;
use crate::auth::now;
use crate::quota;

pub enum Outcome {
    Success { payload: Value, credits: i64, upstream_key_id: i64 },
    /// 上游 4xx（调用方问题），原样带状态码与响应体透传
    Passthrough {
        status: u16,
        payload: Value,
        upstream_key_id: i64,
    },
    /// 全部 key 都不可用
    AllUnavailable,
}

/// 带失败转移地执行一次上游调用。
pub async fn execute(state: &AppState, path: &str, body: &Value) -> Outcome {
    let mut tried: HashSet<i64> = HashSet::new();
    loop {
        let Some((key_id, ciphertext)) = pick_best(state, &tried).await else {
            return Outcome::AllUnavailable;
        };
        let Ok(api_key) = state.crypto.decrypt(&ciphertext) else {
            tried.insert(key_id);
            continue;
        };
        match state.upstream.post_json(path, &api_key, body).await {
            Ok((status, payload)) if (200..300).contains(&status) => {
                let credits = payload["usage"]["credits"].as_i64().unwrap_or(0);
                quota::record_usage(&state.db, key_id, credits).await;
                return Outcome::Success {
                    payload,
                    credits,
                    upstream_key_id: key_id,
                };
            }
            Ok((status, payload)) => match status {
                429 => {
                    mark_cooling(state, key_id).await;
                    tried.insert(key_id);
                }
                432 | 433 => {
                    mark_exhausted(state, key_id).await;
                    tried.insert(key_id);
                }
                401 => {
                    mark_invalid(state, key_id).await;
                    tried.insert(key_id);
                }
                400..=499 => {
                    return Outcome::Passthrough {
                        status,
                        payload,
                        upstream_key_id: key_id,
                    };
                }
                _ => {
                    // 5xx：上游抖动，不动状态，换 key 重试
                    tried.insert(key_id);
                }
            },
            Err(_) => {
                tried.insert(key_id);
            }
        }
    }
}

/// 在健康 key 中选有效剩余额度最多者（平手随机）。
/// limit 未知（NULL，还没轮询过）按无限处理——新 key 优先被试。
async fn pick_best(state: &AppState, exclude: &HashSet<i64>) -> Option<(i64, String)> {
    sweep_expired(&state.db).await;
    let rows = sqlx::query_as::<_, (i64, String, i64, Option<i64>)>(
        "SELECT id, key_ciphertext, usage_cached, limit_cached \
         FROM upstream_keys WHERE status = 'active'",
    )
    .fetch_all(&state.db)
    .await
    .ok()?;

    let mut candidates: Vec<(i64, String, i64)> = rows
        .into_iter()
        .filter(|(id, ..)| !exclude.contains(id))
        .map(|(id, ct, usage, limit)| {
            let remaining = limit.map(|l| l - usage).unwrap_or(i64::MAX);
            (id, ct, remaining)
        })
        .collect();
    let max = candidates.iter().map(|c| c.2).max()?;
    candidates.retain(|c| c.2 == max);
    use rand::seq::IndexedRandom;
    candidates
        .choose(&mut rand::rng())
        .map(|(id, ct, _)| (*id, ct.clone()))
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
    let _ = sqlx::query(
        "UPDATE upstream_keys SET status = 'cooling', cooling_until = ? WHERE id = ?",
    )
    .bind(until)
    .bind(key_id)
    .execute(&state.db)
    .await;
}

async fn mark_exhausted(state: &AppState, key_id: i64) {
    let reset_day = sqlx::query_scalar::<_, i64>("SELECT reset_day FROM upstream_keys WHERE id = ?")
        .bind(key_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or(1);
    let until = next_reset(now(), reset_day);
    let _ = sqlx::query(
        "UPDATE upstream_keys SET status = 'exhausted', exhausted_until = ? WHERE id = ?",
    )
    .bind(until)
    .bind(key_id)
    .execute(&state.db)
    .await;
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
