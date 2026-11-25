use sqlx::SqliteConnection;

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
