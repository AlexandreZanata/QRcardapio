use chrono::NaiveDate;
use sqlx::{Row, SqlitePool};

use crate::domain::affiliate::ReferralStatus;
use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

#[derive(Debug, Clone)]
pub struct ReferralViewRow {
    pub id: String,
    pub referred_account_id: String,
    pub referred_email: String,
    pub establishment_name: Option<String>,
    pub establishment_slug: Option<String>,
    pub status: ReferralStatus,
    pub disqualified_reason: Option<String>,
    pub attributed_at: String,
    pub first_paid_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AffiliateProgramStats {
    pub total_affiliates: i64,
    pub active_affiliates: i64,
    pub total_referrals: i64,
    pub active_referrals: i64,
    pub pending_centavos: i64,
    pub available_centavos: i64,
    pub paid_centavos: i64,
}

pub async fn insert_referral(
    pool: &SqlitePool,
    id: &str,
    affiliate_id: &str,
    referred_account_id: &str,
    now: &str,
) -> AppResult<bool> {
    let res = sqlx::query(
        "INSERT INTO affiliate_referrals (
            id, affiliate_id, referred_account_id, status, attributed_at, updated_at
         ) VALUES (?, ?, ?, 'signed_up', ?, ?)
         ON CONFLICT(referred_account_id) DO NOTHING",
    )
    .bind(id)
    .bind(affiliate_id)
    .bind(referred_account_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn bind_referral_establishment(
    pool: &SqlitePool,
    referred_account_id: &str,
    establishment_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE affiliate_referrals
         SET referred_establishment_id = ?, updated_at = ?
         WHERE referred_account_id = ? AND referred_establishment_id IS NULL",
    )
    .bind(establishment_id)
    .bind(now)
    .bind(referred_account_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn disqualify_referral(
    pool: &SqlitePool,
    affiliate_id: &str,
    referral_id: &str,
    reason: &str,
    now: &str,
) -> AppResult<bool> {
    let mut tx = begin_immediate(pool).await?;
    let res = sqlx::query(
        "UPDATE affiliate_referrals
         SET status = 'disqualified', disqualified_reason = ?, updated_at = ?
         WHERE id = ? AND affiliate_id = ? AND status <> 'disqualified'",
    )
    .bind(reason)
    .bind(now)
    .bind(referral_id)
    .bind(affiliate_id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE affiliate_commissions
         SET status = 'voided', updated_at = ?
         WHERE referral_id = ? AND affiliate_id = ?
           AND entry_kind = 'commission'
           AND status IN ('pending', 'available') AND payout_id IS NULL",
    )
    .bind(now)
    .bind(referral_id)
    .bind(affiliate_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn list_referrals_for_affiliate(
    pool: &SqlitePool,
    affiliate_id: &str,
    limit: i64,
) -> AppResult<Vec<ReferralViewRow>> {
    let rows = sqlx::query(
        "SELECT r.id, r.referred_account_id, acc.email AS referred_email,
                e.name AS establishment_name, e.slug AS establishment_slug,
                r.status, r.disqualified_reason, r.attributed_at, r.first_paid_at
         FROM affiliate_referrals r
         INNER JOIN accounts acc ON acc.id = r.referred_account_id
         LEFT JOIN establishments e ON e.id = r.referred_establishment_id
         WHERE r.affiliate_id = ?
         ORDER BY r.attributed_at DESC
         LIMIT ?",
    )
    .bind(affiliate_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let status_str: String = r.get("status");
            ReferralViewRow {
                id: r.get("id"),
                referred_account_id: r.get("referred_account_id"),
                referred_email: r.get("referred_email"),
                establishment_name: r.get("establishment_name"),
                establishment_slug: r.get("establishment_slug"),
                status: ReferralStatus::parse(&status_str),
                disqualified_reason: r.get("disqualified_reason"),
                attributed_at: r.get("attributed_at"),
                first_paid_at: r.get("first_paid_at"),
            }
        })
        .collect())
}

pub async fn program_overview_stats(
    pool: &SqlitePool,
    env: &str,
    today: NaiveDate,
) -> AppResult<AffiliateProgramStats> {
    let today_str = today.to_string();
    let aff = sqlx::query(
        "SELECT COUNT(*) AS total,
                COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0) AS active
         FROM affiliates",
    )
    .fetch_one(pool)
    .await?;
    let refs = sqlx::query(
        "SELECT COUNT(*) AS total,
                COALESCE(SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END), 0) AS active
         FROM affiliate_referrals",
    )
    .fetch_one(pool)
    .await?;
    let led = sqlx::query(
        "SELECT COALESCE(SUM(CASE WHEN status = 'pending' AND available_on > ? THEN commission_centavos ELSE 0 END), 0) AS pending,
                COALESCE(SUM(CASE WHEN payout_id IS NULL AND (status = 'available' OR (status = 'pending' AND available_on <= ?)) THEN commission_centavos ELSE 0 END), 0) AS available,
                COALESCE(SUM(CASE WHEN status = 'paid' THEN commission_centavos ELSE 0 END), 0) AS paid
         FROM affiliate_commissions
         WHERE environment = ?",
    )
    .bind(&today_str)
    .bind(&today_str)
    .bind(env)
    .fetch_one(pool)
    .await?;

    Ok(AffiliateProgramStats {
        total_affiliates: aff.get("total"),
        active_affiliates: aff.get("active"),
        total_referrals: refs.get("total"),
        active_referrals: refs.get("active"),
        pending_centavos: led.get("pending"),
        available_centavos: led.get("available"),
        paid_centavos: led.get("paid"),
    })
}
