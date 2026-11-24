use chrono::NaiveDate;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::domain::affiliate::{CommissionStatus, CommissionType, EntryKind};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

#[derive(Debug, Clone, Default)]
pub struct AffiliateBalances {
    pub pending_centavos: i64,
    pub available_centavos: i64,
    pub paid_centavos: i64,
    pub voided_centavos: i64,
}

#[derive(Debug, Clone)]
pub struct CommissionLedgerRow {
    pub id: String,
    pub payment_id: String,
    pub establishment_name: String,
    pub entry_kind: EntryKind,
    pub installment_index: i64,
    pub billing_cycle_months: i64,
    pub gross_payment_centavos: i64,
    pub commission_centavos: i64,
    pub snapshot_commission_type: CommissionType,
    pub snapshot_commission_value: i64,
    pub effective_status: CommissionStatus,
    pub available_on: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct PayoutRow {
    pub id: String,
    pub total_centavos: i64,
    pub entries_count: i64,
    pub pix_key_snapshot: String,
    pub reference: String,
    pub notes: String,
    pub paid_by_email: String,
    pub paid_at: String,
}

#[derive(Debug, Clone)]
pub struct SettledPayout {
    pub id: String,
    pub total_centavos: i64,
    pub entries_count: i64,
}

pub async fn affiliate_balances(
    pool: &SqlitePool,
    affiliate_id: &str,
    env: &str,
    today: NaiveDate,
) -> AppResult<AffiliateBalances> {
    let today_str = today.to_string();
    let row = sqlx::query(
        "SELECT COALESCE(SUM(CASE WHEN status = 'pending' AND available_on > ? THEN commission_centavos ELSE 0 END), 0) AS pending,
                COALESCE(SUM(CASE WHEN payout_id IS NULL AND (status = 'available' OR (status = 'pending' AND available_on <= ?)) THEN commission_centavos ELSE 0 END), 0) AS available,
                COALESCE(SUM(CASE WHEN status = 'paid' THEN commission_centavos ELSE 0 END), 0) AS paid,
                COALESCE(SUM(CASE WHEN status = 'voided' AND entry_kind = 'commission' THEN commission_centavos ELSE 0 END), 0) AS voided
         FROM affiliate_commissions
         WHERE affiliate_id = ? AND environment = ?",
    )
    .bind(&today_str)
    .bind(&today_str)
    .bind(affiliate_id)
    .bind(env)
    .fetch_one(pool)
    .await?;

    Ok(AffiliateBalances {
        pending_centavos: row.get("pending"),
        available_centavos: row.get("available"),
        paid_centavos: row.get("paid"),
        voided_centavos: row.get("voided"),
    })
}

pub async fn list_commissions_for_affiliate(
    pool: &SqlitePool,
    affiliate_id: &str,
    env: &str,
    today: NaiveDate,
    limit: i64,
) -> AppResult<Vec<CommissionLedgerRow>> {
    let today_str = today.to_string();
    let rows = sqlx::query(
        "SELECT c.id, c.payment_id, e.name AS establishment_name, c.entry_kind,
                c.installment_index, c.billing_cycle_months, c.gross_payment_centavos,
                c.commission_centavos, c.snapshot_commission_type, c.snapshot_commission_value,
                CASE
                    WHEN c.status = 'pending' AND c.available_on <= ? AND c.payout_id IS NULL
                    THEN 'available'
                    ELSE c.status
                END AS effective_status,
                c.available_on, c.created_at
         FROM affiliate_commissions c
         INNER JOIN establishments e ON e.id = c.establishment_id
         WHERE c.affiliate_id = ? AND c.environment = ?
         ORDER BY c.available_on DESC, c.created_at DESC, c.installment_index DESC
         LIMIT ?",
    )
    .bind(&today_str)
    .bind(affiliate_id)
    .bind(env)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let entry_kind: String = r.get("entry_kind");
            let snap_type: String = r.get("snapshot_commission_type");
            let eff_status: String = r.get("effective_status");
            CommissionLedgerRow {
                id: r.get("id"),
                payment_id: r.get("payment_id"),
                establishment_name: r.get("establishment_name"),
                entry_kind: EntryKind::parse(&entry_kind),
                installment_index: r.get("installment_index"),
                billing_cycle_months: r.get("billing_cycle_months"),
                gross_payment_centavos: r.get("gross_payment_centavos"),
                commission_centavos: r.get("commission_centavos"),
                snapshot_commission_type: CommissionType::parse(&snap_type)
                    .unwrap_or(CommissionType::FixedMonthlyCentavos),
                snapshot_commission_value: r.get("snapshot_commission_value"),
                effective_status: CommissionStatus::parse(&eff_status),
                available_on: r.get("available_on"),
                created_at: r.get("created_at"),
            }
        })
        .collect())
}

pub async fn list_payouts_for_affiliate(
    pool: &SqlitePool,
    affiliate_id: &str,
    env: &str,
    limit: i64,
) -> AppResult<Vec<PayoutRow>> {
    let rows = sqlx::query(
        "SELECT p.id, p.total_centavos, p.entries_count, p.pix_key_snapshot,
                p.reference, p.notes, acc.email AS paid_by_email, p.paid_at
         FROM affiliate_payouts p
         INNER JOIN accounts acc ON acc.id = p.paid_by_account_id
         WHERE p.affiliate_id = ? AND p.environment = ?
         ORDER BY p.paid_at DESC
         LIMIT ?",
    )
    .bind(affiliate_id)
    .bind(env)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PayoutRow {
            id: r.get("id"),
            total_centavos: r.get("total_centavos"),
            entries_count: r.get("entries_count"),
            pix_key_snapshot: r.get("pix_key_snapshot"),
            reference: r.get("reference"),
            notes: r.get("notes"),
            paid_by_email: r.get("paid_by_email"),
            paid_at: r.get("paid_at"),
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn settle_affiliate_payout(
    pool: &SqlitePool,
    affiliate_id: &str,
    env: &str,
    admin_account_id: &str,
    reference: &str,
    notes: &str,
    today: NaiveDate,
    now: &str,
) -> AppResult<SettledPayout> {
    let today_str = today.to_string();
    let mut tx = begin_immediate(pool).await?;

    let pix_key: Option<String> = sqlx::query_scalar("SELECT pix_key FROM affiliates WHERE id = ?")
        .bind(affiliate_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(pix_key) = pix_key else {
        tx.rollback().await?;
        return Err(AppError::NotFound("error.not_found"));
    };

    let summary = sqlx::query(
        "SELECT COUNT(*) AS cnt, COALESCE(SUM(commission_centavos), 0) AS total
         FROM affiliate_commissions
         WHERE affiliate_id = ? AND environment = ? AND payout_id IS NULL
           AND (status = 'available' OR (status = 'pending' AND available_on <= ?))",
    )
    .bind(affiliate_id)
    .bind(env)
    .bind(&today_str)
    .fetch_one(&mut *tx)
    .await?;

    let entries_count: i64 = summary.get("cnt");
    let total_centavos: i64 = summary.get("total");

    if entries_count <= 0 || total_centavos <= 0 {
        tx.rollback().await?;
        return Err(AppError::Validation("error.affiliate_no_payable_balance"));
    }

    let payout_id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO affiliate_payouts (
            id, affiliate_id, environment, total_centavos, entries_count,
            pix_key_snapshot, reference, notes, paid_by_account_id, paid_at, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&payout_id)
    .bind(affiliate_id)
    .bind(env)
    .bind(total_centavos)
    .bind(entries_count)
    .bind(&pix_key)
    .bind(reference)
    .bind(notes)
    .bind(admin_account_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let updated = sqlx::query(
        "UPDATE affiliate_commissions
         SET status = 'paid', payout_id = ?, updated_at = ?
         WHERE affiliate_id = ? AND environment = ? AND payout_id IS NULL
           AND (status = 'available' OR (status = 'pending' AND available_on <= ?))",
    )
    .bind(&payout_id)
    .bind(now)
    .bind(affiliate_id)
    .bind(env)
    .bind(&today_str)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() != entries_count as u64 {
        tx.rollback().await?;
        return Err(AppError::Conflict("error.affiliate_payout_conflict"));
    }

    tx.commit().await?;
    Ok(SettledPayout {
        id: payout_id,
        total_centavos,
        entries_count,
    })
}
