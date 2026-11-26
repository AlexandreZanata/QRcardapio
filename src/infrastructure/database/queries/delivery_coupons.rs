use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Sqlite, SqliteExecutor, SqlitePool, Transaction};
use uuid::Uuid;

use crate::application::auth::is_unique_violation;
use crate::domain::delivery::coupon::{Coupon, CouponKind, ValidatedCoupon};
use crate::error::{AppError, AppResult};

#[derive(sqlx::FromRow)]
struct CouponRow {
    id: String,
    establishment_id: String,
    code: String,
    kind: String,
    percent_bps: Option<i64>,
    amount_centavos: Option<i64>,
    min_subtotal_centavos: i64,
    valid_from: Option<String>,
    valid_until: Option<String>,
    max_redemptions: Option<i64>,
    max_per_customer: Option<i64>,
    redemption_count: i64,
    allow_delivery: bool,
    allow_pickup: bool,
    is_active: bool,
}

impl TryFrom<CouponRow> for Coupon {
    type Error = AppError;

    fn try_from(row: CouponRow) -> AppResult<Self> {
        let parse_dt = |raw: Option<String>| -> AppResult<Option<NaiveDateTime>> {
            raw.map(|s| {
                NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
                    .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M"))
                    .map_err(|_| AppError::Internal("invalid coupon datetime".into()))
            })
            .transpose()
        };
        Ok(Self {
            id: row.id,
            establishment_id: row.establishment_id,
            code: row.code,
            kind: CouponKind::parse(&row.kind)
                .ok_or_else(|| AppError::Internal("invalid coupon kind".into()))?,
            percent_bps: row.percent_bps,
            amount_centavos: row.amount_centavos,
            min_subtotal_centavos: row.min_subtotal_centavos,
            valid_from: parse_dt(row.valid_from)?,
            valid_until: parse_dt(row.valid_until)?,
            max_redemptions: row.max_redemptions,
            max_per_customer: row.max_per_customer,
            redemption_count: row.redemption_count,
            allow_delivery: row.allow_delivery,
            allow_pickup: row.allow_pickup,
            is_active: row.is_active,
        })
    }
}

pub async fn list_for_establishment<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
) -> AppResult<Vec<Coupon>> {
    let rows: Vec<CouponRow> = sqlx::query_as(
        "SELECT id, establishment_id, code, kind, percent_bps, amount_centavos,
                min_subtotal_centavos, valid_from, valid_until, max_redemptions,
                max_per_customer, redemption_count, allow_delivery, allow_pickup, is_active
         FROM coupons WHERE establishment_id = ?
         ORDER BY created_at DESC, code ASC",
    )
    .bind(est_id)
    .fetch_all(executor)
    .await?;
    rows.into_iter().map(Coupon::try_from).collect()
}

pub async fn get_by_code_in<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    code: &str,
) -> AppResult<Option<Coupon>> {
    let row: Option<CouponRow> = sqlx::query_as(
        "SELECT id, establishment_id, code, kind, percent_bps, amount_centavos,
                min_subtotal_centavos, valid_from, valid_until, max_redemptions,
                max_per_customer, redemption_count, allow_delivery, allow_pickup, is_active
         FROM coupons WHERE establishment_id = ? AND code = ?",
    )
    .bind(est_id)
    .bind(code)
    .fetch_optional(executor)
    .await?;
    row.map(Coupon::try_from).transpose()
}

pub async fn count_customer_redemptions_in<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    coupon_id: &str,
    phone_e164: &str,
) -> AppResult<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM coupon_redemptions r
         INNER JOIN customers c ON c.id = r.customer_id AND c.establishment_id = r.establishment_id
         WHERE r.establishment_id = ? AND r.coupon_id = ? AND c.phone_e164 = ? AND r.status = 'consumed'",
    )
    .bind(est_id)
    .bind(coupon_id)
    .bind(phone_e164)
    .fetch_one(executor)
    .await?;
    Ok(count)
}

fn format_dt(dt: Option<NaiveDateTime>) -> Option<String> {
    dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
}

pub async fn create(
    pool: &SqlitePool,
    est_id: &str,
    coupon: &ValidatedCoupon,
    now: &str,
) -> AppResult<Coupon> {
    let id = Uuid::now_v7().to_string();
    let row: CouponRow = sqlx::query_as(
        "INSERT INTO coupons (
            id, establishment_id, code, kind, percent_bps, amount_centavos,
            min_subtotal_centavos, valid_from, valid_until, max_redemptions,
            max_per_customer, allow_delivery, allow_pickup, is_active, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id, establishment_id, code, kind, percent_bps, amount_centavos,
                   min_subtotal_centavos, valid_from, valid_until, max_redemptions,
                   max_per_customer, redemption_count, allow_delivery, allow_pickup, is_active",
    )
    .bind(&id)
    .bind(est_id)
    .bind(&coupon.code)
    .bind(coupon.kind.as_str())
    .bind(coupon.percent_bps)
    .bind(coupon.amount_centavos)
    .bind(coupon.min_subtotal_centavos)
    .bind(format_dt(coupon.valid_from))
    .bind(format_dt(coupon.valid_until))
    .bind(coupon.max_redemptions)
    .bind(coupon.max_per_customer)
    .bind(coupon.allow_delivery)
    .bind(coupon.allow_pickup)
    .bind(coupon.is_active)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(map_coupon_err)?;
    Coupon::try_from(row)
}

pub async fn update(
    pool: &SqlitePool,
    est_id: &str,
    coupon_id: &str,
    coupon: &ValidatedCoupon,
    now: &str,
) -> AppResult<Coupon> {
    let row: Option<CouponRow> = sqlx::query_as(
        "UPDATE coupons SET
            code = ?, kind = ?, percent_bps = ?, amount_centavos = ?,
            min_subtotal_centavos = ?, valid_from = ?, valid_until = ?,
            max_redemptions = ?, max_per_customer = ?, allow_delivery = ?,
            allow_pickup = ?, is_active = ?, updated_at = ?
         WHERE id = ? AND establishment_id = ?
         RETURNING id, establishment_id, code, kind, percent_bps, amount_centavos,
                   min_subtotal_centavos, valid_from, valid_until, max_redemptions,
                   max_per_customer, redemption_count, allow_delivery, allow_pickup, is_active",
    )
    .bind(&coupon.code)
    .bind(coupon.kind.as_str())
    .bind(coupon.percent_bps)
    .bind(coupon.amount_centavos)
    .bind(coupon.min_subtotal_centavos)
    .bind(format_dt(coupon.valid_from))
    .bind(format_dt(coupon.valid_until))
    .bind(coupon.max_redemptions)
    .bind(coupon.max_per_customer)
    .bind(coupon.allow_delivery)
    .bind(coupon.allow_pickup)
    .bind(coupon.is_active)
    .bind(now)
    .bind(coupon_id)
    .bind(est_id)
    .fetch_optional(pool)
    .await
    .map_err(map_coupon_err)?;
    row.map(Coupon::try_from)
        .transpose()?
        .ok_or(AppError::NotFound("error.not_found"))
}

pub async fn delete(pool: &SqlitePool, est_id: &str, coupon_id: &str) -> AppResult<bool> {
    let result = sqlx::query("DELETE FROM coupons WHERE id = ? AND establishment_id = ?")
        .bind(coupon_id)
        .bind(est_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Atomically consumes one redemption for `order_id` inside the checkout transaction.
/// Returns `false` when the coupon is unavailable or either limit has been reached.
pub(crate) async fn consume_in(
    tx: &mut Transaction<'_, Sqlite>,
    est_id: &str,
    coupon_id: &str,
    order_id: &str,
    customer_id: &str,
    now: DateTime<Utc>,
) -> AppResult<bool> {
    let ts = now.to_rfc3339();
    let updated = sqlx::query(
        "UPDATE coupons
         SET redemption_count = redemption_count + 1, updated_at = ?
         WHERE id = ? AND establishment_id = ? AND is_active = 1
           AND (max_redemptions IS NULL OR redemption_count < max_redemptions)
           AND EXISTS (SELECT 1 FROM order_customers c
               WHERE c.establishment_id = coupons.establishment_id
                 AND c.order_id = ? AND c.customer_id = ?)
           AND (max_per_customer IS NULL OR max_per_customer > (
               SELECT COUNT(*) FROM coupon_redemptions r
               WHERE r.establishment_id = coupons.establishment_id AND r.coupon_id = coupons.id
                 AND r.customer_id = ? AND r.status = 'consumed'))",
    )
    .bind(&ts)
    .bind(coupon_id)
    .bind(est_id)
    .bind(order_id)
    .bind(customer_id)
    .bind(customer_id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO coupon_redemptions (id, establishment_id, coupon_id, order_id, customer_id, status, created_at)
         VALUES (?, ?, ?, ?, ?, 'consumed', ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(est_id)
    .bind(coupon_id)
    .bind(order_id)
    .bind(customer_id)
    .bind(&ts)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

/// Releases any consumed coupon redemption when an order transitions to `rejected` or `cancelled`.
pub(crate) async fn release_for_order_in(
    tx: &mut Transaction<'_, Sqlite>,
    est_id: &str,
    order_id: &str,
    at: DateTime<Utc>,
) -> AppResult<()> {
    let ts = at.to_rfc3339();
    let released_coupon: Option<String> = sqlx::query_scalar(
        "UPDATE coupon_redemptions
         SET status = 'released', released_at = ?
         WHERE establishment_id = ? AND order_id = ? AND status = 'consumed'
         RETURNING coupon_id",
    )
    .bind(&ts)
    .bind(est_id)
    .bind(order_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(coupon_id) = released_coupon {
        sqlx::query(
            "UPDATE coupons
             SET redemption_count = MAX(0, redemption_count - 1), updated_at = ?
             WHERE id = ? AND establishment_id = ?",
        )
        .bind(&ts)
        .bind(coupon_id)
        .bind(est_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn map_coupon_err(err: sqlx::Error) -> AppError {
    if err
        .as_database_error()
        .is_some_and(|db| db.is_check_violation())
    {
        return AppError::Validation("delivery.invalid_coupon");
    }
    let err = AppError::from(err);
    if is_unique_violation(&err) {
        AppError::Conflict("delivery.duplicate_coupon")
    } else {
        err
    }
}
