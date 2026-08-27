//! 额度簿记（ADR-0002）：GET /usage 周期轮询为主数据源，usage.credits 本地扣减兜底。

use sqlx::SqlitePool;

use crate::app::AppState;
use crate::auth::now;

/// 后台轮询任务：按 AppState.quota_poll_interval 周期刷新所有非禁用 key 的额度。
pub fn spawn_poller(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(state.quota_poll_interval).await;
            poll_once(&state).await;
        }
    });
}

async fn poll_once(state: &AppState) {
    let keys = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, key_ciphertext FROM upstream_keys WHERE status != 'disabled'",
    )
    .fetch_all(&state.db)
    .await;
    let Ok(keys) = keys else { return };

    for (id, ciphertext) in keys {
        let Ok(api_key) = state.crypto.decrypt(&ciphertext) else { continue };
        match state.upstream.fetch_usage(&api_key).await {
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

/// 请求成功后本地扣减该 key 的已用量（票 07/08 在管道里调用）。
#[allow(dead_code)]
pub async fn record_usage(db: &SqlitePool, upstream_key_id: i64, credits: i64) {
    let _ = sqlx::query(
        "UPDATE upstream_keys SET usage_cached = usage_cached + ? WHERE id = ?",
    )
    .bind(credits)
    .bind(upstream_key_id)
    .execute(db)
    .await;
}
