use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

use super::{customer_addresses, customers};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

#[derive(Serialize)]
pub struct CustomerExport {
    customer: customers::Customer,
    addresses: Vec<customer_addresses::CustomerAddress>,
    orders: Vec<Value>,
}

/// Use a consistent read transaction so an export sees a profile and orders from one revision.
pub async fn export(
    pool: &SqlitePool,
    establishment: &str,
    customer: &str,
) -> AppResult<CustomerExport> {
    let mut tx = pool.begin().await?;
    let profile = customers::find_in(&mut tx, establishment, customer).await?;
    let addresses = customer_addresses::list_in(&mut tx, establishment, customer).await?;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT json_object(
            'public_id', o.public_id, 'order_number', o.order_number, 'business_date', o.business_date,
            'channel', o.channel, 'fulfillment', o.fulfillment, 'state', o.state, 'placed_at', o.placed_at,
            'items_subtotal_centavos', o.items_subtotal_centavos, 'discount_centavos', o.discount_centavos,
            'delivery_fee_centavos', o.delivery_fee_centavos, 'total_centavos', o.total_centavos,
            'delivery', json((SELECT json_object('recipient_name', d.recipient_name, 'recipient_phone', d.recipient_phone,
                 'street', d.street, 'street_number', d.street_number, 'complement', d.complement,
                 'neighborhood', d.neighborhood_label, 'city', d.city, 'uf', d.uf, 'cep', d.cep, 'reference', d.reference)
                 FROM order_delivery_details d WHERE d.order_id = o.id AND d.establishment_id = o.establishment_id)),
            'items', json((SELECT json_group_array(json_object('name', i.name, 'quantity', i.quantity,
                 'variation', i.variation_name, 'unit_price_centavos', i.unit_price_centavos,
                 'line_total_centavos', i.line_total_centavos,
                 'extras', json((SELECT json_group_array(json_object('name', a.name, 'group', a.group_name,
                    'quantity', a.quantity, 'unit_price_centavos', a.unit_price_centavos) ORDER BY a.option_id)
                    FROM order_item_options a WHERE a.order_item_id = i.id AND a.establishment_id = i.establishment_id)))
                 ORDER BY i.position) FROM order_items i WHERE i.order_id = o.id AND i.establishment_id = o.establishment_id)))
         FROM order_customers c JOIN orders o ON o.id = c.order_id AND o.establishment_id = c.establishment_id
         WHERE c.customer_id = ? AND c.establishment_id = ? ORDER BY o.placed_at, o.id",
    ).bind(customer).bind(establishment).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let orders = rows
        .iter()
        .map(|row| serde_json::from_str(row))
        .collect::<Result<_, _>>()
        .map_err(|error| AppError::Internal(error.to_string()))?;
    Ok(CustomerExport {
        customer: profile,
        addresses,
        orders,
    })
}

/// Erase private fields and detach orders while preserving their commercial facts and event log.
pub async fn erase(
    pool: &SqlitePool,
    establishment: &str,
    customer: &str,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let erased = sqlx::query(
        "UPDATE customers SET name = NULL, phone_e164 = NULL, email = NULL, updated_at = ?, erased_at = ?
         WHERE id = ? AND establishment_id = ? AND erased_at IS NULL",
    ).bind(now).bind(now).bind(customer).bind(establishment).execute(&mut *tx).await?;
    if erased.rows_affected() != 1 {
        return Err(AppError::NotFound("error.not_found"));
    }
    sqlx::query("DELETE FROM customer_addresses WHERE customer_id = ? AND establishment_id = ?")
        .bind(customer)
        .bind(establishment)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE order_delivery_details SET recipient_name = NULL, recipient_phone = NULL,
             street = NULL, street_number = NULL, complement = NULL, city = NULL, uf = NULL,
             cep = NULL, reference = NULL, erased_at = ?
         WHERE establishment_id = ? AND order_id IN (
             SELECT order_id FROM order_customers WHERE customer_id = ? AND establishment_id = ?)",
    )
    .bind(now)
    .bind(establishment)
    .bind(customer)
    .bind(establishment)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM order_customers WHERE customer_id = ? AND establishment_id = ?")
        .bind(customer)
        .bind(establishment)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
