use chrono::NaiveDate;
use sqlx::{Row, SqlitePool};

use crate::domain::affiliate::{
    Affiliate, AffiliateProgramSettings, AffiliateStatus, CommissionType,
};
use crate::error::AppResult;

pub use super::affiliate_referrals::*;

#[derive(Debug, Clone)]
pub struct AffiliateSummaryRow {
    pub affiliate: Affiliate,
    pub account_email: String,
    pub referrals_total: i64,
    pub referrals_active: i64,
    pub pending_centavos: i64,
    pub available_centavos: i64,
    pub paid_centavos: i64,
}

fn row_to_affiliate(row: &sqlx::sqlite::SqliteRow) -> Affiliate {
    let status_str: String = row.get("status");
    let kind_str: String = row.get("commission_type");
    Affiliate {
        id: row.get("id"),
        account_id: row.get("account_id"),
        code: row.get("code"),
        status: AffiliateStatus::parse(&status_str).unwrap_or(AffiliateStatus::Revoked),
        commission_type: CommissionType::parse(&kind_str)
            .unwrap_or(CommissionType::FixedMonthlyCentavos),
        commission_value: row.get("commission_value"),
        hold_days: row.get("hold_days"),
        pix_key: row.get("pix_key"),
        notes: row.get("notes"),
        click_count: row.get("click_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn get_program_settings(pool: &SqlitePool) -> AppResult<AffiliateProgramSettings> {
    let row = sqlx::query(
        "SELECT self_signup_enabled, default_commission_type, default_commission_value,
                default_hold_days, cookie_ttl_days, updated_at
         FROM affiliate_program_settings WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    let kind_str: String = row.get("default_commission_type");
    let enabled: i64 = row.get("self_signup_enabled");
    Ok(AffiliateProgramSettings {
        self_signup_enabled: enabled != 0,
        default_commission_type: CommissionType::parse(&kind_str)
            .unwrap_or(CommissionType::FixedMonthlyCentavos),
        default_commission_value: row.get("default_commission_value"),
        default_hold_days: row.get("default_hold_days"),
        cookie_ttl_days: row.get("cookie_ttl_days"),
        updated_at: row.get("updated_at"),
    })
}

pub async fn update_program_settings(
    pool: &SqlitePool,
    settings: &AffiliateProgramSettings,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE affiliate_program_settings
         SET self_signup_enabled = ?, default_commission_type = ?,
             default_commission_value = ?, default_hold_days = ?,
             cookie_ttl_days = ?, updated_at = ?
         WHERE id = 1",
    )
    .bind(i64::from(settings.self_signup_enabled))
    .bind(settings.default_commission_type.as_str())
    .bind(settings.default_commission_value)
    .bind(settings.default_hold_days)
    .bind(settings.cookie_ttl_days)
    .bind(&settings.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_affiliate_by_account(
    pool: &SqlitePool,
    account_id: &str,
) -> AppResult<Option<Affiliate>> {
    let row = sqlx::query("SELECT * FROM affiliates WHERE account_id = ?")
        .bind(account_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_affiliate(&r)))
}

pub async fn find_affiliate_by_id(pool: &SqlitePool, id: &str) -> AppResult<Option<Affiliate>> {
    let row = sqlx::query("SELECT * FROM affiliates WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_affiliate(&r)))
}

pub async fn find_affiliate_by_code(pool: &SqlitePool, code: &str) -> AppResult<Option<Affiliate>> {
    let row = sqlx::query("SELECT * FROM affiliates WHERE code = ?")
        .bind(code)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_affiliate(&r)))
}

pub async fn find_affiliate_with_email_by_id(
    pool: &SqlitePool,
    id: &str,
) -> AppResult<Option<(Affiliate, String)>> {
    let row = sqlx::query(
        "SELECT a.*, acc.email AS account_email
         FROM affiliates a
         INNER JOIN accounts acc ON acc.id = a.account_id
         WHERE a.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (row_to_affiliate(&r), r.get("account_email"))))
}

pub async fn find_affiliate_with_email_by_code(
    pool: &SqlitePool,
    code: &str,
) -> AppResult<Option<(Affiliate, String)>> {
    let row = sqlx::query(
        "SELECT a.*, acc.email AS account_email
         FROM affiliates a
         INNER JOIN accounts acc ON acc.id = a.account_id
         WHERE a.code = ?",
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (row_to_affiliate(&r), r.get("account_email"))))
}

pub async fn insert_affiliate(pool: &SqlitePool, aff: &Affiliate) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO affiliates (
            id, account_id, code, status, commission_type, commission_value,
            hold_days, pix_key, notes, click_count, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&aff.id)
    .bind(&aff.account_id)
    .bind(&aff.code)
    .bind(aff.status.as_str())
    .bind(aff.commission_type.as_str())
    .bind(aff.commission_value)
    .bind(aff.hold_days)
    .bind(&aff.pix_key)
    .bind(&aff.notes)
    .bind(aff.click_count)
    .bind(&aff.created_at)
    .bind(&aff.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_affiliate_admin(
    pool: &SqlitePool,
    id: &str,
    code: &str,
    status: AffiliateStatus,
    commission_type: CommissionType,
    commission_value: i64,
    hold_days: i64,
    pix_key: &str,
    notes: &str,
    now: &str,
) -> AppResult<bool> {
    let res = sqlx::query(
        "UPDATE affiliates
         SET code = ?, status = ?, commission_type = ?, commission_value = ?,
             hold_days = ?, pix_key = ?, notes = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(code)
    .bind(status.as_str())
    .bind(commission_type.as_str())
    .bind(commission_value)
    .bind(hold_days)
    .bind(pix_key)
    .bind(notes)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn update_affiliate_pix(
    pool: &SqlitePool,
    id: &str,
    pix_key: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query("UPDATE affiliates SET pix_key = ?, updated_at = ? WHERE id = ?")
        .bind(pix_key)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn increment_affiliate_clicks(pool: &SqlitePool, affiliate_id: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE affiliates SET click_count = click_count + 1
         WHERE id = ? AND status = 'active'",
    )
    .bind(affiliate_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_affiliates_summary(
    pool: &SqlitePool,
    env: &str,
    today: NaiveDate,
) -> AppResult<Vec<AffiliateSummaryRow>> {
    let today_str = today.to_string();
    let rows = sqlx::query(
        "SELECT a.*, acc.email AS account_email,
                COALESCE(ref_stats.referrals_total, 0) AS referrals_total,
                COALESCE(ref_stats.referrals_active, 0) AS referrals_active,
                COALESCE(led.pending_centavos, 0) AS pending_centavos,
                COALESCE(led.available_centavos, 0) AS available_centavos,
                COALESCE(led.paid_centavos, 0) AS paid_centavos
         FROM affiliates a
         INNER JOIN accounts acc ON acc.id = a.account_id
         LEFT JOIN (
             SELECT affiliate_id,
                    COUNT(*) AS referrals_total,
                    SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) AS referrals_active
             FROM affiliate_referrals
             GROUP BY affiliate_id
         ) ref_stats ON ref_stats.affiliate_id = a.id
         LEFT JOIN (
             SELECT affiliate_id,
                    SUM(CASE WHEN status = 'pending' AND available_on > ? THEN commission_centavos ELSE 0 END) AS pending_centavos,
                    SUM(CASE WHEN payout_id IS NULL AND (status = 'available' OR (status = 'pending' AND available_on <= ?)) THEN commission_centavos ELSE 0 END) AS available_centavos,
                    SUM(CASE WHEN status = 'paid' THEN commission_centavos ELSE 0 END) AS paid_centavos
             FROM affiliate_commissions
             WHERE environment = ?
             GROUP BY affiliate_id
         ) led ON led.affiliate_id = a.id
         ORDER BY a.created_at DESC",
    )
    .bind(&today_str)
    .bind(&today_str)
    .bind(env)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| AffiliateSummaryRow {
            account_email: r.get("account_email"),
            referrals_total: r.get("referrals_total"),
            referrals_active: r.get("referrals_active"),
            pending_centavos: r.get("pending_centavos"),
            available_centavos: r.get("available_centavos"),
            paid_centavos: r.get("paid_centavos"),
            affiliate: row_to_affiliate(&r),
        })
        .collect())
}
