use sqlx::{Sqlite, Transaction};

use crate::error::AppResult;

pub(crate) async fn defer(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    order_id: &str,
    retry_at_ms: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO order_expiry_retries (establishment_id, order_id, retry_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT (establishment_id, order_id) DO UPDATE SET retry_at = excluded.retry_at
         WHERE order_id = ?2 AND establishment_id = ?1",
    )
    .bind(establishment_id)
    .bind(order_id)
    .bind(retry_at_ms)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
