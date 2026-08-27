//! 选路与失败转移。票 07 只有 pick_any_active；额度感知与状态机在票 08 补全。

use sqlx::SqlitePool;

/// 任意一个 active 上游密钥（最早添加者）。
pub async fn pick_any_active(db: &SqlitePool) -> Option<(i64, String)> {
    sqlx::query_as::<_, (i64, String)>(
        "SELECT id, key_ciphertext FROM upstream_keys WHERE status = 'active' ORDER BY id LIMIT 1",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}
