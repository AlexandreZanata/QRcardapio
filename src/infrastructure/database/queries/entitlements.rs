use chrono::NaiveDate;
use sqlx::{Sqlite, SqliteExecutor, Transaction};

use crate::error::AppResult;

/// One coverage row of an establishment: a paid contract capability period
/// (`commercial_entitlement_periods`) or an unrevoked explicit grant.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CoverageRow {
    /// `contract` or `grant`.
    pub source: String,
    pub source_id: String,
    pub grant_kind: Option<String>,
    pub offer_code: String,
    /// Set for contract rows; grants cover every capability of `offer_code`.
    pub capability: Option<String>,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub renewing: bool,
}

/// Every coverage source of one establishment in a single round trip. Contract periods are
/// limited to `env`; with `None` (billing not enforced) only grants are returned.
pub async fn coverage_rows<'e>(
    executor: impl SqliteExecutor<'e>,
    env: Option<&str>,
    establishment_id: &str,
) -> AppResult<Vec<CoverageRow>> {
    Ok(sqlx::query_as(
        "SELECT 'contract' AS source, contract_id AS source_id, NULL AS grant_kind, offer_code,
                capability, starts_on, ends_on, renewing
         FROM commercial_entitlement_periods
         WHERE environment = ? AND establishment_id = ?
         UNION ALL
         SELECT 'grant', id, grant_kind, offer_code, NULL, starts_on, ends_on, 0
         FROM entitlement_grants
         WHERE establishment_id = ? AND revoked_at IS NULL",
    )
    .bind(env)
    .bind(establishment_id)
    .bind(establishment_id)
    .fetch_all(executor)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GrantRow {
    pub id: String,
    pub establishment_id: String,
    pub offer_kind: String,
    pub offer_code: String,
    pub grant_kind: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: String,
    pub revoked_at: Option<String>,
}

pub struct NewGrant<'a> {
    pub id: &'a str,
    pub establishment_id: &'a str,
    pub offer_kind: &'a str,
    pub offer_code: &'a str,
    pub grant_kind: &'a str,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: &'a str,
    pub granted_by: &'a str,
    pub now: &'a str,
}

/// Inserts a grant. Returns `false` when the one-trial-per-offer rule rejects it.
pub async fn insert_grant(
    tx: &mut Transaction<'_, Sqlite>,
    grant: &NewGrant<'_>,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT INTO entitlement_grants (
            id, establishment_id, offer_kind, offer_code, grant_kind, starts_on, ends_on, reason,
            granted_by_account_id, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT DO NOTHING",
    )
    .bind(grant.id)
    .bind(grant.establishment_id)
    .bind(grant.offer_kind)
    .bind(grant.offer_code)
    .bind(grant.grant_kind)
    .bind(grant.starts_on)
    .bind(grant.ends_on)
    .bind(grant.reason)
    .bind(grant.granted_by)
    .bind(grant.now)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Revokes a grant of the given establishment. Returns the revoked row, if any.
pub async fn revoke_grant(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    grant_id: &str,
    revoked_by: &str,
    now: &str,
) -> AppResult<Option<GrantRow>> {
    Ok(sqlx::query_as(
        "UPDATE entitlement_grants SET revoked_at = ?, revoked_by_account_id = ?
         WHERE id = ? AND establishment_id = ? AND revoked_at IS NULL
         RETURNING id, establishment_id, offer_kind, offer_code, grant_kind, starts_on, ends_on,
                   reason, revoked_at",
    )
    .bind(now)
    .bind(revoked_by)
    .bind(grant_id)
    .bind(establishment_id)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Current operational status (`active` when no row exists).
pub async fn operational_status<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
) -> AppResult<String> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM establishment_operational_states WHERE establishment_id = ?",
    )
    .bind(establishment_id)
    .fetch_optional(executor)
    .await?;
    Ok(status.unwrap_or_else(|| "active".to_owned()))
}

/// Upserts the operational status and returns the previous one.
pub async fn set_operational_status(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    status: &str,
    reason: &str,
    changed_by: &str,
    now: &str,
) -> AppResult<String> {
    let previous = operational_status(&mut **tx, establishment_id).await?;
    sqlx::query(
        "INSERT INTO establishment_operational_states
            (establishment_id, status, reason, changed_by_account_id, changed_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(establishment_id) DO UPDATE SET status = excluded.status,
            reason = excluded.reason, changed_by_account_id = excluded.changed_by_account_id,
            changed_at = excluded.changed_at",
    )
    .bind(establishment_id)
    .bind(status)
    .bind(reason)
    .bind(changed_by)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(previous)
}
