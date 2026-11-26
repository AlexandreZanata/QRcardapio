use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow)]
pub struct OrderExportRow {
    pub order_number: i64,
    pub business_date: String,
    pub public_id: String,
    pub channel: String,
    pub fulfillment: String,
    pub state: String,
    pub items_subtotal_centavos: i64,
    pub discount_centavos: i64,
    pub delivery_fee_centavos: i64,
    pub total_centavos: i64,
    pub payment_method: String,
    pub payment_state: String,
    pub neighborhood_label: String,
    pub placed_at: String,
}

pub async fn load(
    pool: &SqlitePool,
    establishment_id: &str,
    state_filter: Option<&str>,
    date_filter: Option<&str>,
) -> AppResult<Vec<OrderExportRow>> {
    const MAX_EXPORT_ROWS: i64 = 10_000;
    let rows = sqlx::query_as(
        "SELECT o.order_number, o.business_date, o.public_id, o.channel, o.fulfillment,
                o.state, o.items_subtotal_centavos, o.discount_centavos, o.delivery_fee_centavos,
                o.total_centavos, COALESCE(p.method, '') AS payment_method,
                COALESCE(p.state, '') AS payment_state,
                COALESCE(m.neighborhood_label, '') AS neighborhood_label, o.placed_at
         FROM orders o
         LEFT JOIN order_payments p ON p.establishment_id = o.establishment_id AND p.order_id = o.id
              AND p.id = (
                  SELECT latest.id FROM order_payments latest
                  WHERE latest.establishment_id = o.establishment_id AND latest.order_id = o.id
                  ORDER BY latest.created_at DESC, latest.id DESC LIMIT 1
              )
         LEFT JOIN delivery_order_metadata m
              ON m.establishment_id = o.establishment_id AND m.order_id = o.id
         WHERE o.establishment_id = ?
           AND o.catalog_channel = 'storefront' AND o.channel IN ('delivery', 'pickup')
           AND (? IS NULL OR o.state = ?)
           AND (? IS NULL OR o.business_date = ?)
         ORDER BY o.placed_at DESC, o.id DESC
         LIMIT ?",
    )
    .bind(establishment_id)
    .bind(state_filter)
    .bind(state_filter)
    .bind(date_filter)
    .bind(date_filter)
    .bind(MAX_EXPORT_ROWS + 1)
    .fetch_all(pool)
    .await?;
    if rows.len() as i64 > MAX_EXPORT_ROWS {
        return Err(AppError::Validation("delivery.orders.export_limit"));
    }
    Ok(rows)
}
