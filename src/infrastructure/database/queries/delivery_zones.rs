use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use crate::domain::delivery::zone::{DeliveryZone, ValidatedDeliveryZone};
use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow)]
struct ZoneRow {
    id: String,
    establishment_id: String,
    neighborhood: String,
    city: String,
    normalized_key: String,
    fee_centavos: i64,
    min_order_centavos: Option<i64>,
    free_delivery_from_centavos: Option<i64>,
    eta_min_minutes: i64,
    eta_max_minutes: i64,
    is_active: bool,
    position: i64,
}

impl From<ZoneRow> for DeliveryZone {
    fn from(row: ZoneRow) -> Self {
        Self {
            id: row.id,
            establishment_id: row.establishment_id,
            neighborhood: row.neighborhood,
            city: row.city,
            normalized_key: row.normalized_key,
            fee_centavos: row.fee_centavos,
            min_order_centavos: row.min_order_centavos,
            free_delivery_from_centavos: row.free_delivery_from_centavos,
            eta_min_minutes: row.eta_min_minutes,
            eta_max_minutes: row.eta_max_minutes,
            is_active: row.is_active,
            position: row.position,
        }
    }
}

pub async fn list_for_establishment<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
) -> AppResult<Vec<DeliveryZone>> {
    let rows: Vec<ZoneRow> = sqlx::query_as(
        "SELECT id, establishment_id, neighborhood, city, normalized_key, fee_centavos,
                min_order_centavos, free_delivery_from_centavos, eta_min_minutes,
                eta_max_minutes, is_active, position
         FROM delivery_zones WHERE establishment_id = ?
         ORDER BY position ASC, neighborhood ASC, id ASC",
    )
    .bind(est_id)
    .fetch_all(executor)
    .await?;
    Ok(rows.into_iter().map(DeliveryZone::from).collect())
}

pub async fn list_active_for_establishment<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
) -> AppResult<Vec<DeliveryZone>> {
    let rows: Vec<ZoneRow> = sqlx::query_as(
        "SELECT id, establishment_id, neighborhood, city, normalized_key, fee_centavos,
                min_order_centavos, free_delivery_from_centavos, eta_min_minutes,
                eta_max_minutes, is_active, position
         FROM delivery_zones WHERE establishment_id = ? AND is_active = 1
         ORDER BY position ASC, neighborhood ASC, id ASC",
    )
    .bind(est_id)
    .fetch_all(executor)
    .await?;
    Ok(rows.into_iter().map(DeliveryZone::from).collect())
}

pub async fn get_in<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    zone_id: &str,
) -> AppResult<Option<DeliveryZone>> {
    let row: Option<ZoneRow> = sqlx::query_as(
        "SELECT id, establishment_id, neighborhood, city, normalized_key, fee_centavos,
                min_order_centavos, free_delivery_from_centavos, eta_min_minutes,
                eta_max_minutes, is_active, position
         FROM delivery_zones WHERE id = ? AND establishment_id = ?",
    )
    .bind(zone_id)
    .bind(est_id)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(DeliveryZone::from))
}

pub async fn create(
    pool: &SqlitePool,
    est_id: &str,
    zone: &ValidatedDeliveryZone,
    now: &str,
) -> AppResult<DeliveryZone> {
    let id = Uuid::now_v7().to_string();
    let row: ZoneRow = sqlx::query_as(
        "INSERT INTO delivery_zones (
            id, establishment_id, neighborhood, city, normalized_key, fee_centavos,
            min_order_centavos, free_delivery_from_centavos, eta_min_minutes,
            eta_max_minutes, is_active, position, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id, establishment_id, neighborhood, city, normalized_key, fee_centavos,
                   min_order_centavos, free_delivery_from_centavos, eta_min_minutes,
                   eta_max_minutes, is_active, position",
    )
    .bind(&id)
    .bind(est_id)
    .bind(&zone.neighborhood)
    .bind(&zone.city)
    .bind(&zone.normalized_key)
    .bind(zone.fee_centavos)
    .bind(zone.min_order_centavos)
    .bind(zone.free_delivery_from_centavos)
    .bind(zone.eta_min_minutes)
    .bind(zone.eta_max_minutes)
    .bind(zone.is_active)
    .bind(zone.position)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(map_zone_err)?;
    Ok(DeliveryZone::from(row))
}

pub async fn update(
    pool: &SqlitePool,
    est_id: &str,
    zone_id: &str,
    zone: &ValidatedDeliveryZone,
    now: &str,
) -> AppResult<DeliveryZone> {
    let row: Option<ZoneRow> = sqlx::query_as(
        "UPDATE delivery_zones SET
            neighborhood = ?, city = ?, normalized_key = ?, fee_centavos = ?,
            min_order_centavos = ?, free_delivery_from_centavos = ?,
            eta_min_minutes = ?, eta_max_minutes = ?, is_active = ?, position = ?,
            updated_at = ?
         WHERE id = ? AND establishment_id = ?
         RETURNING id, establishment_id, neighborhood, city, normalized_key, fee_centavos,
                   min_order_centavos, free_delivery_from_centavos, eta_min_minutes,
                   eta_max_minutes, is_active, position",
    )
    .bind(&zone.neighborhood)
    .bind(&zone.city)
    .bind(&zone.normalized_key)
    .bind(zone.fee_centavos)
    .bind(zone.min_order_centavos)
    .bind(zone.free_delivery_from_centavos)
    .bind(zone.eta_min_minutes)
    .bind(zone.eta_max_minutes)
    .bind(zone.is_active)
    .bind(zone.position)
    .bind(now)
    .bind(zone_id)
    .bind(est_id)
    .fetch_optional(pool)
    .await
    .map_err(map_zone_err)?;
    row.map(DeliveryZone::from)
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn delete(pool: &SqlitePool, est_id: &str, zone_id: &str) -> AppResult<bool> {
    let result = sqlx::query("DELETE FROM delivery_zones WHERE id = ? AND establishment_id = ?")
        .bind(zone_id)
        .bind(est_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}

fn map_zone_err(err: sqlx::Error) -> AppError {
    if err
        .as_database_error()
        .is_some_and(|error| error.is_unique_violation())
    {
        AppError::Conflict("delivery.duplicate_zone")
    } else {
        err.into()
    }
}
