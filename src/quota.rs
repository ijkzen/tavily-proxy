//! 额度簿记：Tavily 以 GET /usage 周期轮询为主数据源（ADR-0002），
//! 请求响应里的用量本地扣减兜底；Exa 无公开余额接口，按「每月 10 美元」
//! 本地记账，到每月 1 号重置（ADR-0004）。

use sqlx::SqlitePool;
use time::{Month, OffsetDateTime};

use crate::app::AppState;
use crate::auth::now;
use crate::provider::Kind;
use crate::request_logs;

/// Exa 每月本地记账的额度（美元），见 ADR-0004。
pub const EXA_MONTHLY_CREDITS: f64 = 10.0;

/// 后台轮询任务：按 AppState.quota_poll_interval 周期刷新所有非禁用 Tavily key 的额度，
/// 顺带清理超过保留期的请求日志（默认 30 天，LOG_RETENTION_DAYS 可配）。
/// Exa 密钥没有可查的余额接口，跳过轮询（本地记账即可）。
pub fn spawn_poller(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(state.quota_poll_interval).await;
            poll_once(&state).await;
            request_logs::cleanup_expired(&state.db, state.log_retention).await;
        }
    });
}

async fn poll_once(state: &AppState) {
    let keys = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, key_ciphertext FROM upstream_keys \
         WHERE status != 'disabled' AND kind = 'tavily'",
    )
    .fetch_all(&state.db)
    .await;
    let Ok(keys) = keys else { return };

    for (id, ciphertext) in keys {
        let Ok(api_key) = state.crypto.decrypt(&ciphertext) else {
            continue;
        };
        let Some(provider) = state.providers.get(Kind::Tavily as usize) else {
            continue;
        };
        match state
            .upstream
            .fetch_usage(&provider.base_url, &api_key)
            .await
        {
            Ok(usage) => {
                // 账号粒度建模（ADR-0002）：限额取 key.limit ?? account.plan_limit，
                // 已用量取两者较大者（账号口径是硬约束）。
                let used = usage.key.usage.max(usage.account.plan_usage);
                let limit = usage.key.limit.or(usage.account.plan_limit);
                let _ = sqlx::query(
                    "UPDATE upstream_keys \
                     SET usage_cached = ?, limit_cached = ?, usage_fetched_at = ? \
                     WHERE id = ?",
                )
                .bind(used)
                .bind(limit)
                .bind(now())
                .bind(id)
                .execute(&state.db)
                .await;
            }
            Err(err) => {
                // 静默降级：保留本地数据，只留告警（ADR-0002）
                let _ = sqlx::query(
                    "INSERT INTO alerts (upstream_key_id, kind, message, created_at) \
                     VALUES (?, 'quota_poll_failed', ?, ?)",
                )
                .bind(id)
                .bind(format!("{err:#}"))
                .bind(now())
                .execute(&state.db)
                .await;
            }
        }
    }
}

/// 请求成功后按 kind 本地扣减该 key 的已用量。
/// - tavily：usage_cached 累加 credits（整数）。
/// - exa：usage_cached 累加美元（浮点），并在跨过每月重置点（quota_reset_at）
///   时把已用量回滚到「本月实际」——本地记账，供展示与排序。
pub async fn record_usage(db: &SqlitePool, upstream_key_id: i64, amount: f64) {
    let row = sqlx::query_as::<_, (String, Option<i64>)>(
        "SELECT kind, quota_reset_at FROM upstream_keys WHERE id = ?",
    )
    .bind(upstream_key_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let Some((kind, _)) = row else { return };

    match kind.as_str() {
        "exa" => {
            let _ = sqlx::query(
                "UPDATE upstream_keys SET usage_cached = ROUND(usage_cached + ?, 4) WHERE id = ?",
            )
            .bind(amount)
            .bind(upstream_key_id)
            .execute(db)
            .await;
            // 惰性重置：当前记账周期的用量已超出本月额度 → 重算到最近的重置点
            let _ = sqlx::query(
                "UPDATE upstream_keys SET usage_cached = ? \
                 WHERE id = ? AND usage_cached > ? AND quota_reset_at IS NOT NULL",
            )
            .bind(EXA_MONTHLY_CREDITS)
            .bind(upstream_key_id)
            .bind(EXA_MONTHLY_CREDITS)
            .execute(db)
            .await;
        }
        _ => {
            let _ = sqlx::query(
                "UPDATE upstream_keys SET usage_cached = usage_cached + ? WHERE id = ?",
            )
            .bind(amount.round() as i64)
            .bind(upstream_key_id)
            .execute(db)
            .await;
        }
    }
}

/// Exa 密钥创建时初始化本地记账：每月额度 10 美元，重置点为下月 1 号 0 点（UTC）。
pub async fn init_exa_quota(db: &SqlitePool, key_id: i64) {
    let reset_at = next_month_utc_zero();
    let _ = sqlx::query(
        "UPDATE upstream_keys SET usage_cached = 0, limit_cached = ?, quota_reset_at = ? \
         WHERE id = ?",
    )
    .bind(EXA_MONTHLY_CREDITS)
    .bind(reset_at)
    .bind(key_id)
    .execute(db)
    .await;
}

/// 下月 1 号 0 点（UTC）的 unix 秒。
fn next_month_utc_zero() -> i64 {
    let now = OffsetDateTime::from_unix_timestamp(now()).unwrap();
    let (year, month) = match now.month() {
        Month::December => (now.year() + 1, Month::January),
        m => (now.year(), m.next()),
    };
    time::Date::from_calendar_date(year, month, 1)
        .unwrap()
        .with_hms(0, 0, 0)
        .unwrap()
        .assume_utc()
        .unix_timestamp()
}
