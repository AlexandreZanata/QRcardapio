use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::domain::commerce::customer::CustomerProfile;
use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub phone_e164: String,
    pub email: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub const PAGE_SIZE: i64 = 40;

pub async fn page(pool: &SqlitePool, establishment: &str, page: i64) -> AppResult<Vec<Customer>> {
    Ok(sqlx::query_as(
        "SELECT id, name, phone_e164, email, created_at, updated_at FROM customers
         WHERE establishment_id = ? AND erased_at IS NULL ORDER BY name COLLATE NOCASE, id
         LIMIT ? OFFSET ?",
    )
    .bind(establishment)
    .bind(PAGE_SIZE + 1)
    .bind((page - 1) * PAGE_SIZE)
    .fetch_all(pool)
    .await?)
}

pub(super) async fn find_in(
    conn: &mut SqliteConnection,
    establishment: &str,
    id: &str,
) -> AppResult<Customer> {
    sqlx::query_as(
        "SELECT id, name, phone_e164, email, created_at, updated_at FROM customers
         WHERE id = ? AND establishment_id = ? AND erased_at IS NULL",
    )
    .bind(id)
    .bind(establishment)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn find(pool: &SqlitePool, establishment: &str, id: &str) -> AppResult<Customer> {
    find_in(&mut *pool.acquire().await?, establishment, id).await
}

pub async fn create(
    pool: &SqlitePool,
    establishment: &str,
    profile: &CustomerProfile,
    now: &str,
) -> AppResult<String> {
    let id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO customers (id, establishment_id, name, phone_e164, email, created_at, updated_at)
         VALUES (?, ?, ?, ?, NULLIF(?, ''), ?, ?)",
    ).bind(&id).bind(establishment).bind(&profile.name).bind(&profile.phone_e164).bind(&profile.email)
        .bind(now).bind(now).execute(pool).await.map_err(map_write)?;
    Ok(id)
}

pub async fn update(
    pool: &SqlitePool,
    establishment: &str,
    id: &str,
    profile: &CustomerProfile,
    now: &str,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE customers SET name = ?, phone_e164 = ?, email = NULLIF(?, ''), updated_at = ?
         WHERE id = ? AND establishment_id = ? AND erased_at IS NULL",
    )
    .bind(&profile.name)
    .bind(&profile.phone_e164)
    .bind(&profile.email)
    .bind(now)
    .bind(id)
    .bind(establishment)
    .execute(pool)
    .await
    .map_err(map_write)?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

fn map_write(error: sqlx::Error) -> AppError {
    if error
        .as_database_error()
        .is_some_and(|error| error.is_unique_violation())
    {
        AppError::Conflict("commerce.customer.duplicate_phone")
    } else {
        error.into()
    }
}
