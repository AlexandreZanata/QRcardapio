use chrono::{DateTime, Utc};
use sqlx::{Sqlite, SqliteExecutor, SqlitePool, Transaction};

use crate::domain::commerce::order::{Fulfillment, Order, OrderChannel};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeliveryOrderMetadata {
    pub establishment_id: String,
    pub order_id: String,
    pub cancel_token_hash: String,
    pub zone_id: Option<String>,
    pub neighborhood_label: String,
    pub coupon_id: Option<String>,
    pub coupon_code: Option<String>,
    pub order_note: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrderItemOptionRecord {
    pub order_item_id: String,
    pub option_id: String,
    pub group_name: String,
    pub group_name_en: String,
    pub group_name_es: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub unit_price_centavos: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrderItemRecord {
    pub id: String,
    pub product_id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub variation_name: Option<String>,
    pub variation_name_en: Option<String>,
    pub variation_name_es: Option<String>,
    pub unit_price_centavos: i64,
    pub quantity: i64,
    pub line_total_centavos: i64,
    pub note: String,
}

pub(crate) async fn insert_item_note_in(
    tx: &mut Transaction<'_, Sqlite>,
    est_id: &str,
    order_id: &str,
    position: usize,
    note: &str,
) -> AppResult<()> {
    let inserted = sqlx::query(
        "INSERT INTO delivery_order_item_notes (establishment_id, order_item_id, note)
         SELECT establishment_id, id, ? FROM order_items
         WHERE establishment_id = ? AND order_id = ? AND position = ?",
    )
    .bind(note)
    .bind(est_id)
    .bind(order_id)
    .bind(position as i64)
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() != 1 {
        return Err(AppError::Internal("missing Delivery order item".into()));
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrderDeliveryAddressRecord {
    pub recipient_name: String,
    pub recipient_phone: String,
    pub street: String,
    pub street_number: String,
    pub complement: String,
    pub neighborhood_label: String,
    pub city: String,
    pub uf: String,
    pub cep: String,
    pub reference: String,
}

pub(crate) async fn insert_metadata_in(
    tx: &mut Transaction<'_, Sqlite>,
    meta: &DeliveryOrderMetadata,
    pix_receipt_key: Option<&str>,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO delivery_order_metadata (
            establishment_id, order_id, cancel_token_hash, zone_id, neighborhood_label,
            coupon_id, coupon_code, order_note, pix_receipt_key, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&meta.establishment_id)
    .bind(&meta.order_id)
    .bind(&meta.cancel_token_hash)
    .bind(&meta.zone_id)
    .bind(&meta.neighborhood_label)
    .bind(&meta.coupon_id)
    .bind(&meta.coupon_code)
    .bind(&meta.order_note)
    .bind(pix_receipt_key)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The caller must verify the original checkout attempt before retrieving its receipt.
pub(crate) async fn get_pix_receipt_key<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    public_id: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT m.pix_receipt_key FROM orders o
         INNER JOIN delivery_order_metadata m
            ON m.establishment_id = o.establishment_id AND m.order_id = o.id
         WHERE o.establishment_id = ? AND o.public_id = ? AND m.pix_receipt_key IS NOT NULL",
    )
    .bind(est_id)
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn get_metadata<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    order_id: &str,
) -> AppResult<Option<DeliveryOrderMetadata>> {
    Ok(sqlx::query_as(
        "SELECT establishment_id, order_id, cancel_token_hash, zone_id, neighborhood_label,
                coupon_id, coupon_code, order_note
         FROM delivery_order_metadata
         WHERE order_id = ? AND establishment_id = ?",
    )
    .bind(order_id)
    .bind(est_id)
    .fetch_optional(executor)
    .await?)
}

pub(crate) async fn acceptance_estimates(
    pool: &SqlitePool,
    est_id: &str,
    order_id: &str,
    delivery: bool,
) -> AppResult<(Option<i64>, i64)> {
    Ok(sqlx::query_as(
        "SELECT (SELECT default_prep_minutes FROM storefronts WHERE establishment_id = ?1),
                COALESCE((
                    SELECT z.eta_max_minutes FROM delivery_order_metadata m
                    JOIN delivery_zones z ON z.id = m.zone_id AND z.establishment_id = m.establishment_id
                    WHERE m.order_id = ?2 AND m.establishment_id = ?1 AND ?3
                ), 0)",
    )
    .bind(est_id)
    .bind(order_id)
    .bind(delivery)
    .fetch_one(pool)
    .await?)
}

pub async fn get_customer_id_for_order<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    order_id: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT customer_id FROM order_customers WHERE order_id = ? AND establishment_id = ?",
    )
    .bind(order_id)
    .bind(est_id)
    .fetch_optional(executor)
    .await?)
}

#[derive(sqlx::FromRow)]
struct PublicOrderRow {
    id: String,
    public_id: String,
    establishment_id: String,
    order_number: i64,
    business_date: chrono::NaiveDate,
    channel: String,
    fulfillment: String,
    state: String,
    version: i64,
    catalog_channel: String,
    catalog_version: i64,
    items_subtotal_centavos: i64,
    discount_centavos: i64,
    delivery_fee_centavos: i64,
    total_centavos: i64,
    placed_at: DateTime<Utc>,
    acceptance_deadline_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    accepted_at: Option<DateTime<Utc>>,
    promised_at: Option<DateTime<Utc>>,
    preparing_at: Option<DateTime<Utc>>,
    ready_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
    reason_code: Option<String>,
}

impl TryFrom<PublicOrderRow> for Order {
    type Error = AppError;

    fn try_from(row: PublicOrderRow) -> AppResult<Self> {
        Ok(Self {
            id: row.id,
            public_id: row.public_id,
            establishment_id: row.establishment_id,
            order_number: row.order_number,
            business_date: row.business_date,
            channel: OrderChannel::parse(&row.channel)
                .ok_or_else(|| AppError::Internal("invalid stored order channel".into()))?,
            fulfillment: Fulfillment::parse(&row.fulfillment)
                .ok_or_else(|| AppError::Internal("invalid stored order fulfillment".into()))?,
            state: row
                .state
                .parse()
                .map_err(|_| AppError::Internal("invalid stored order state".into()))?,
            version: row.version,
            catalog_channel: row.catalog_channel,
            catalog_version: row.catalog_version,
            items_subtotal_centavos: row.items_subtotal_centavos,
            discount_centavos: row.discount_centavos,
            delivery_fee_centavos: row.delivery_fee_centavos,
            total_centavos: row.total_centavos,
            placed_at: row.placed_at,
            acceptance_deadline_at: row.acceptance_deadline_at,
            updated_at: row.updated_at,
            accepted_at: row.accepted_at,
            promised_at: row.promised_at,
            preparing_at: row.preparing_at,
            ready_at: row.ready_at,
            completed_at: row.completed_at,
            rejected_at: row.rejected_at,
            cancelled_at: row.cancelled_at,
            reason_code: row.reason_code,
        })
    }
}

pub async fn find_order_by_public_id(
    pool: &SqlitePool,
    est_id: &str,
    public_id: &str,
) -> AppResult<Option<Order>> {
    let row: Option<PublicOrderRow> = sqlx::query_as(
        "SELECT id, public_id, establishment_id, order_number, business_date, channel, fulfillment,
                state, version, catalog_channel, catalog_version, items_subtotal_centavos,
                discount_centavos, delivery_fee_centavos, total_centavos, placed_at,
                acceptance_deadline_at, updated_at, accepted_at, promised_at, preparing_at,
                ready_at, completed_at, rejected_at, cancelled_at, reason_code
         FROM orders WHERE public_id = ? AND establishment_id = ?
           AND catalog_channel = 'storefront' AND channel IN ('delivery', 'pickup')",
    )
    .bind(public_id)
    .bind(est_id)
    .fetch_optional(pool)
    .await?;
    row.map(Order::try_from).transpose()
}

pub async fn list_orders_for_center(
    pool: &SqlitePool,
    est_id: &str,
    state_filter: Option<&str>,
    date_filter: Option<&str>,
) -> AppResult<Vec<Order>> {
    let rows: Vec<PublicOrderRow> = sqlx::query_as(
        "SELECT id, public_id, establishment_id, order_number, business_date, channel, fulfillment,
                state, version, catalog_channel, catalog_version, items_subtotal_centavos,
                discount_centavos, delivery_fee_centavos, total_centavos, placed_at,
                acceptance_deadline_at, updated_at, accepted_at, promised_at, preparing_at,
                ready_at, completed_at, rejected_at, cancelled_at, reason_code
         FROM orders
         WHERE establishment_id = ?
           AND catalog_channel = 'storefront' AND channel IN ('delivery', 'pickup')
           AND (? IS NULL OR state = ?)
           AND (? IS NULL OR business_date = ?)
         ORDER BY placed_at DESC, id DESC
         LIMIT 100",
    )
    .bind(est_id)
    .bind(state_filter)
    .bind(state_filter)
    .bind(date_filter)
    .bind(date_filter)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(Order::try_from).collect()
}

pub async fn center_etag_summary(
    pool: &SqlitePool,
    est_id: &str,
    state_filter: Option<&str>,
    date_filter: Option<&str>,
) -> AppResult<String> {
    let (count, order_versions, orders_updated_at, pay_ver, customer_links, customers_updated_at): (
        i64,
        i64,
        String,
        i64,
        i64,
        String,
    ) = sqlx::query_as(
        "WITH center_orders AS MATERIALIZED (
            SELECT id, establishment_id, version, updated_at
            FROM orders
            WHERE establishment_id = ?
              AND catalog_channel = 'storefront' AND channel IN ('delivery', 'pickup')
              AND (? IS NULL OR state = ?)
              AND (? IS NULL OR business_date = ?)
            ORDER BY placed_at DESC, id DESC
            LIMIT 100
         ),
         customer_changes AS (
            SELECT COUNT(*) AS links, COALESCE(MAX(c.updated_at), '') AS updated_at
            FROM center_orders o
            INNER JOIN order_customers link ON link.establishment_id = o.establishment_id AND link.order_id = o.id
            INNER JOIN customers c ON c.establishment_id = link.establishment_id AND c.id = link.customer_id
            WHERE c.erased_at IS NULL
         )
         SELECT COUNT(*), COALESCE(SUM(version), 0), COALESCE(MAX(updated_at), ''),
                (SELECT COALESCE(SUM(p.version), 0)
                 FROM center_orders o
                 INNER JOIN order_payments p ON p.establishment_id = o.establishment_id AND p.order_id = o.id),
                (SELECT links FROM customer_changes),
                (SELECT updated_at FROM customer_changes)
         FROM center_orders",
    )
    .bind(est_id)
    .bind(state_filter)
    .bind(state_filter)
    .bind(date_filter)
    .bind(date_filter)
    .fetch_one(pool)
    .await?;
    Ok(format!(
        "\"oc-{count}-{order_versions}-{orders_updated_at}-{pay_ver}-{customer_links}-{customers_updated_at}\""
    ))
}

pub async fn list_order_items(
    pool: &SqlitePool,
    est_id: &str,
    order_id: &str,
) -> AppResult<Vec<OrderItemRecord>> {
    Ok(sqlx::query_as(
        "SELECT i.id, i.product_id, i.name, i.name_en, i.name_es, i.variation_name, i.variation_name_en,
                i.variation_name_es, i.unit_price_centavos, i.quantity, i.line_total_centavos,
                COALESCE(n.note, '') AS note
         FROM order_items i
         LEFT JOIN delivery_order_item_notes n
           ON n.establishment_id = i.establishment_id AND n.order_item_id = i.id
         WHERE i.order_id = ? AND i.establishment_id = ?
         ORDER BY i.position ASC, i.id ASC",
    )
    .bind(order_id)
    .bind(est_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_order_item_options(
    pool: &SqlitePool,
    est_id: &str,
    order_id: &str,
) -> AppResult<Vec<OrderItemOptionRecord>> {
    Ok(sqlx::query_as(
        "SELECT o.order_item_id, o.option_id, o.group_name, o.group_name_en, o.group_name_es,
                o.name, o.name_en, o.name_es, o.unit_price_centavos, o.quantity
         FROM order_item_options o
         INNER JOIN order_items i ON i.id = o.order_item_id AND i.establishment_id = o.establishment_id
         WHERE i.order_id = ? AND o.establishment_id = ?
         ORDER BY o.order_item_id ASC, o.option_id ASC",
    )
    .bind(order_id)
    .bind(est_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_delivery_address(
    pool: &SqlitePool,
    est_id: &str,
    order_id: &str,
) -> AppResult<Option<OrderDeliveryAddressRecord>> {
    Ok(sqlx::query_as(
        "SELECT COALESCE(recipient_name, '') AS recipient_name,
                COALESCE(recipient_phone, '') AS recipient_phone,
                COALESCE(street, '') AS street,
                COALESCE(street_number, '') AS street_number,
                COALESCE(complement, '') AS complement,
                neighborhood_label,
                COALESCE(city, '') AS city,
                COALESCE(uf, '') AS uf,
                COALESCE(cep, '') AS cep,
                COALESCE(reference, '') AS reference
         FROM order_delivery_details
         WHERE order_id = ? AND establishment_id = ?",
    )
    .bind(order_id)
    .bind(est_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn is_phone_blocked<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    phone_e164: &str,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM delivery_customer_blocks WHERE establishment_id = ? AND phone_e164 = ?)",
    )
    .bind(est_id)
    .bind(phone_e164)
    .fetch_one(executor)
    .await?)
}

pub async fn set_phone_blocked(
    pool: &SqlitePool,
    est_id: &str,
    phone_e164: &str,
    block: bool,
    reason: &str,
    actor_account_id: &str,
    now: &str,
) -> AppResult<()> {
    if block {
        sqlx::query(
            "INSERT INTO delivery_customer_blocks (establishment_id, phone_e164, reason, blocked_by_account_id, blocked_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(establishment_id, phone_e164) DO UPDATE SET
                reason = excluded.reason,
                blocked_by_account_id = excluded.blocked_by_account_id,
                blocked_at = excluded.blocked_at",
        )
        .bind(est_id)
        .bind(phone_e164)
        .bind(reason)
        .bind(actor_account_id)
        .bind(now)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM delivery_customer_blocks WHERE establishment_id = ? AND phone_e164 = ?",
        )
        .bind(est_id)
        .bind(phone_e164)
        .execute(pool)
        .await?;
    }
    Ok(())
}
