use sqlx::{SqliteConnection, SqliteExecutor};

use crate::domain::commerce::order::Order;
use crate::error::AppResult;

pub(crate) async fn link(
    conn: &mut SqliteConnection,
    order: &Order,
    customer_id: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO order_customers (establishment_id, order_id, customer_id) VALUES (?, ?, ?)",
    )
    .bind(&order.establishment_id)
    .bind(&order.id)
    .bind(customer_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_contact<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    order_id: &str,
) -> AppResult<Option<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT c.name, c.phone_e164
         FROM order_customers link
         INNER JOIN customers c ON c.establishment_id = link.establishment_id AND c.id = link.customer_id
         WHERE link.establishment_id = ? AND link.order_id = ? AND c.erased_at IS NULL",
    )
    .bind(est_id)
    .bind(order_id)
    .fetch_optional(executor)
    .await?)
}
