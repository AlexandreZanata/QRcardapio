use sqlx::{Row, SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::domain::commerce::customer::PostalAddress;
use crate::error::{AppError, AppResult};

#[derive(serde::Serialize)]
pub struct CustomerAddress {
    pub id: String,
    #[serde(flatten)]
    pub address: PostalAddress,
}

pub(super) async fn list_in(
    conn: &mut SqliteConnection,
    establishment: &str,
    customer: &str,
) -> AppResult<Vec<CustomerAddress>> {
    let rows = sqlx::query(
        "SELECT id, street, street_number, complement, neighborhood, city, uf, cep, reference
         FROM customer_addresses WHERE customer_id = ? AND establishment_id = ? ORDER BY id",
    )
    .bind(customer)
    .bind(establishment)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| CustomerAddress {
            id: row.get("id"),
            address: PostalAddress {
                street: row.get("street"),
                street_number: row.get("street_number"),
                complement: row.get("complement"),
                neighborhood: row.get("neighborhood"),
                city: row.get("city"),
                uf: row.get("uf"),
                cep: row.get("cep"),
                reference: row.get("reference"),
            },
        })
        .collect())
}

pub async fn list(
    pool: &SqlitePool,
    establishment: &str,
    customer: &str,
) -> AppResult<Vec<CustomerAddress>> {
    list_in(&mut *pool.acquire().await?, establishment, customer).await
}

pub async fn save(
    pool: &SqlitePool,
    establishment: &str,
    customer: &str,
    id: Option<&str>,
    address: &PostalAddress,
) -> AppResult<()> {
    let new_id;
    let (id, sql) = match id {
        Some(id) => (
            id,
            "UPDATE customer_addresses SET street = ?, street_number = ?, complement = ?,
                 neighborhood = ?, city = ?, uf = ?, cep = ?, reference = ?
                 WHERE id = ? AND customer_id = ? AND establishment_id = ?",
        ),
        None => {
            new_id = Uuid::now_v7().to_string();
            (
                new_id.as_str(),
                "INSERT INTO customer_addresses (street, street_number, complement,
                 neighborhood, city, uf, cep, reference, id, establishment_id, customer_id)
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, c.establishment_id, c.id FROM customers c
                 WHERE c.id = ? AND c.establishment_id = ? AND c.erased_at IS NULL",
            )
        }
    };
    let result = sqlx::query(sql)
        .bind(&address.street)
        .bind(&address.street_number)
        .bind(&address.complement)
        .bind(&address.neighborhood)
        .bind(&address.city)
        .bind(&address.uf)
        .bind(&address.cep)
        .bind(&address.reference)
        .bind(id)
        .bind(customer)
        .bind(establishment)
        .execute(pool)
        .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

pub async fn delete(
    pool: &SqlitePool,
    establishment: &str,
    customer: &str,
    id: &str,
) -> AppResult<()> {
    let deleted = sqlx::query(
        "DELETE FROM customer_addresses WHERE id = ? AND establishment_id = ? AND customer_id = ?",
    )
    .bind(id)
    .bind(establishment)
    .bind(customer)
    .execute(pool)
    .await?;
    if deleted.rows_affected() != 1 {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}
