use sqlx::SqliteConnection;

use crate::domain::commerce::customer::{CustomerProfile, PostalAddress};
use crate::domain::commerce::order::Order;
use crate::error::AppResult;

pub(crate) async fn insert(
    conn: &mut SqliteConnection,
    order: &Order,
    profile: &CustomerProfile,
    address: &PostalAddress,
    neighborhood_label: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO order_delivery_details (establishment_id, order_id, recipient_name, recipient_phone,
            street, street_number, complement, neighborhood_label, city, uf, cep, reference)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&order.establishment_id)
    .bind(&order.id)
    .bind(&profile.name)
    .bind(&profile.phone_e164)
    .bind(&address.street)
    .bind(&address.street_number)
    .bind(&address.complement)
    .bind(neighborhood_label)
    .bind(&address.city)
    .bind(&address.uf)
    .bind((!address.cep.is_empty()).then_some(address.cep.as_str()))
    .bind(&address.reference)
    .execute(&mut *conn)
    .await?;
    // Free-form address text must not become a neighborhood label retained after erasure.
    sqlx::query(
        "INSERT INTO order_delivery_neighborhoods (establishment_id, order_id, neighborhood)
         VALUES (?, ?, ?)",
    )
    .bind(&order.establishment_id)
    .bind(&order.id)
    .bind(&address.neighborhood)
    .execute(conn)
    .await?;
    Ok(())
}
