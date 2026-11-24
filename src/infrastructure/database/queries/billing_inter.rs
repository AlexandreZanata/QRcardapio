use chrono::NaiveDate;
use sqlx::{Sqlite, SqliteExecutor, Transaction};

use crate::error::AppResult;

/// One Inter charge (BolePix) for one subscription cycle.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InterCharge {
    pub our_number: String,
    pub billing_reference: String,
    pub establishment_id: String,
    pub cycle_index: i64,
    pub due_date: NaiveDate,
    pub amount_centavos: i64,
    pub request_code: Option<String>,
    pub status: String,
    pub pix_copy_paste: Option<String>,
    pub digitable_line: Option<String>,
    pub updated_at: String,
}

const COLUMNS: &str = "our_number, billing_reference, establishment_id, cycle_index, due_date,
    amount_centavos, request_code, status, pix_copy_paste, digitable_line, updated_at";

pub struct NewCharge<'a> {
    pub our_number: &'a str,
    pub billing_reference: &'a str,
    pub establishment_id: &'a str,
    pub cycle_index: i64,
    pub due_date: NaiveDate,
    pub amount_centavos: i64,
    pub now: &'a str,
}

/// Reserves the charge of a cycle before calling Inter. Returns `false` when the cycle already
/// has a charge (retries reuse it instead of issuing a second one).
pub async fn reserve(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    charge: &NewCharge<'_>,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT INTO billing_inter_charges (
            environment, our_number, billing_reference, establishment_id, cycle_index, due_date,
            amount_centavos, status, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, 'reserved', ?, ?)
         ON CONFLICT DO NOTHING",
    )
    .bind(env)
    .bind(charge.our_number)
    .bind(charge.billing_reference)
    .bind(charge.establishment_id)
    .bind(charge.cycle_index)
    .bind(charge.due_date)
    .bind(charge.amount_centavos)
    .bind(charge.now)
    .bind(charge.now)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn find_by_cycle<'e>(
    executor: impl SqliteExecutor<'e>,
    reference: &str,
    cycle_index: i64,
) -> AppResult<Option<InterCharge>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM billing_inter_charges WHERE billing_reference = ? AND cycle_index = ?"
    )))
    .bind(reference)
    .bind(cycle_index)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_by_request<'e>(
    executor: impl SqliteExecutor<'e>,
    env: &str,
    request_code: &str,
) -> AppResult<Option<InterCharge>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM billing_inter_charges WHERE environment = ? AND request_code = ?"
    )))
    .bind(env)
    .bind(request_code)
    .fetch_optional(executor)
    .await?)
}

/// Stores Inter's request code once the charge exists at the bank.
pub async fn attach_request<'e>(
    executor: impl SqliteExecutor<'e>,
    env: &str,
    our_number: &str,
    request_code: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_inter_charges SET request_code = ?, status = 'issued', updated_at = ?
         WHERE environment = ? AND our_number = ? AND request_code IS NULL",
    )
    .bind(request_code)
    .bind(now)
    .bind(env)
    .bind(our_number)
    .execute(executor)
    .await?;
    Ok(())
}

pub struct ChargeState<'a> {
    pub request_code: &'a str,
    pub status: &'a str,
    pub pix_copy_paste: Option<&'a str>,
    pub digitable_line: Option<&'a str>,
    pub now: &'a str,
}

pub async fn update_state<'e>(
    executor: impl SqliteExecutor<'e>,
    env: &str,
    state: &ChargeState<'_>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE billing_inter_charges
         SET status = ?, pix_copy_paste = COALESCE(?, pix_copy_paste),
             digitable_line = COALESCE(?, digitable_line), updated_at = ?
         WHERE environment = ? AND request_code = ?",
    )
    .bind(state.status)
    .bind(state.pix_copy_paste)
    .bind(state.digitable_line)
    .bind(state.now)
    .bind(env)
    .bind(state.request_code)
    .execute(executor)
    .await?;
    Ok(())
}

/// Most recent charge of an establishment (billing page).
pub async fn latest_for_establishment<'e>(
    executor: impl SqliteExecutor<'e>,
    env: &str,
    establishment_id: &str,
) -> AppResult<Option<InterCharge>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM billing_inter_charges
         WHERE environment = ? AND establishment_id = ?
         ORDER BY due_date DESC, cycle_index DESC LIMIT 1"
    )))
    .bind(env)
    .bind(establishment_id)
    .fetch_optional(executor)
    .await?)
}

/// Charges that may still change at the bank (reconciliation polling).
pub async fn open_charges<'e>(
    executor: impl SqliteExecutor<'e>,
    env: &str,
    limit: i64,
) -> AppResult<Vec<InterCharge>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM billing_inter_charges
         WHERE environment = ? AND status IN ('reserved', 'issued', 'A_RECEBER', 'ATRASADO', 'EM_PROCESSAMENTO')
         ORDER BY due_date ASC LIMIT ?"
    )))
    .bind(env)
    .bind(limit)
    .fetch_all(executor)
    .await?)
}

/// Charges of one subscription that can still be cancelled at the bank.
pub async fn cancellable_for_reference<'e>(
    executor: impl SqliteExecutor<'e>,
    reference: &str,
) -> AppResult<Vec<InterCharge>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM billing_inter_charges
         WHERE billing_reference = ? AND status IN ('reserved', 'issued', 'A_RECEBER', 'ATRASADO')"
    )))
    .bind(reference)
    .fetch_all(executor)
    .await?)
}

/// Active Inter subscription with the data needed to schedule its next cycle.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RenewalCandidate {
    pub reference: String,
    pub establishment_id: String,
    pub plan_months: i64,
    pub price_centavos: i64,
    pub anchor_due_date: NaiveDate,
    pub last_cycle: i64,
    pub last_status: String,
}

/// Inter subscriptions that still renew, with their first due date and latest cycle.
pub async fn renewal_candidates<'e>(
    executor: impl SqliteExecutor<'e>,
    env: &str,
) -> AppResult<Vec<RenewalCandidate>> {
    Ok(sqlx::query_as(
        "SELECT c.reference, c.establishment_id, c.plan_months, c.price_centavos,
                first.due_date AS anchor_due_date, last.cycle_index AS last_cycle,
                last.status AS last_status
         FROM billing_checkouts c
         JOIN billing_inter_charges first
           ON first.billing_reference = c.reference AND first.cycle_index = 0
         JOIN billing_inter_charges last
           ON last.billing_reference = c.reference
          AND last.cycle_index = (SELECT MAX(cycle_index) FROM billing_inter_charges
                                  WHERE billing_reference = c.reference)
         WHERE c.environment = ? AND c.provider = 'inter' AND c.subscription_status = 'active'",
    )
    .bind(env)
    .fetch_all(executor)
    .await?)
}
