use chrono::NaiveDate;
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

use crate::domain::affiliate::{
    AffiliateStatus, CommissionType, canonical_email_identity, compute_installment_schedule,
};
use crate::error::AppResult;

pub use super::affiliate_payouts::*;

/// Synchronizes all payments belonging to a subscription inside the active billing transaction.
/// Called when a subscription is linked to a checkout (webhook or manual reconciliation).
pub async fn sync_subscription_commissions(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    subscription_id: &str,
    today: NaiveDate,
    now: &str,
) -> AppResult<()> {
    let payment_ids: Vec<String> = sqlx::query_scalar(
        "SELECT payment_id FROM billing_payments
         WHERE environment = ? AND subscription_id = ?",
    )
    .bind(env)
    .bind(subscription_id)
    .fetch_all(&mut **tx)
    .await?;

    for payment_id in payment_ids {
        sync_payment_commissions(tx, env, &payment_id, today, now).await?;
    }
    Ok(())
}

/// Authoritatively reconciles affiliate commission ledger entries for one Asaas payment
/// inside the caller's `BEGIN IMMEDIATE` transaction.
pub async fn sync_payment_commissions(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    payment_id: &str,
    today: NaiveDate,
    now: &str,
) -> AppResult<()> {
    let payment_row = sqlx::query(
        "SELECT p.payment_id, p.subscription_id, p.due_date, p.confirmed_centavos,
                c.establishment_id, c.plan_months, c.price_centavos,
                COALESCE(k.affiliate_eligible_centavos, c.price_centavos) AS eligible_centavos
         FROM billing_payments p
         INNER JOIN billing_checkouts c
             ON c.environment = p.environment AND c.subscription_id = p.subscription_id
         LEFT JOIN commercial_contracts k ON k.billing_reference = c.reference
         WHERE p.environment = ? AND p.payment_id = ?",
    )
    .bind(env)
    .bind(payment_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(payment_row) = payment_row else {
        return Ok(());
    };

    let subscription_id: String = payment_row.get("subscription_id");
    let due_date: NaiveDate = payment_row.get("due_date");
    let confirmed_centavos: Option<i64> = payment_row.get("confirmed_centavos");
    let establishment_id: String = payment_row.get("establishment_id");
    let plan_months: i64 = payment_row.get("plan_months");
    let price_centavos: i64 = payment_row.get("price_centavos");
    // SaaS revenue eligible for commissions (contract snapshot). Add-ons with pass-through
    // costs are excluded; legacy contracts are fully eligible.
    let eligible_centavos: i64 = payment_row.get("eligible_centavos");
    let today_str = today.to_string();

    let valid_confirmed = confirmed_centavos.is_some_and(|amt| amt == price_centavos);
    if !valid_confirmed {
        reverse_payment_commissions(tx, env, payment_id, &today_str, now).await?;
        return Ok(());
    }

    let existing_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM affiliate_commissions
         WHERE environment = ? AND payment_id = ? AND entry_kind = 'commission'",
    )
    .bind(env)
    .bind(payment_id)
    .fetch_one(&mut **tx)
    .await?;

    if existing_count > 0 {
        restore_voided_payment_commissions(tx, env, payment_id, now).await?;
        return Ok(());
    }

    let first_evaluation = sqlx::query(
        "INSERT INTO affiliate_evaluated_payments (environment, payment_id, evaluated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(environment, payment_id) DO NOTHING",
    )
    .bind(env)
    .bind(payment_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    if first_evaluation.rows_affected() == 0 {
        return Ok(());
    }

    let Some(target) = resolve_eligible_referral(tx, &establishment_id, now).await? else {
        return Ok(());
    };

    let schedule = compute_installment_schedule(
        target.commission_type,
        target.commission_value,
        plan_months,
        eligible_centavos.min(price_centavos),
        due_date,
        target.hold_days,
    );
    if schedule.is_empty() {
        return Ok(());
    }

    for inst in schedule {
        let available_on_str = inst.available_on.to_string();
        sqlx::query(
            "INSERT INTO affiliate_commissions (
                id, environment, affiliate_id, referral_id, establishment_id,
                payment_id, subscription_id, entry_kind, installment_index,
                billing_cycle_months, gross_payment_centavos, commission_centavos,
                snapshot_commission_type, snapshot_commission_value, status,
                available_on, payout_id, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, 'commission', ?, ?, ?, ?, ?, ?, 'pending', ?, NULL, ?, ?)
             ON CONFLICT(environment, payment_id, installment_index, entry_kind) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(env)
        .bind(&target.affiliate_id)
        .bind(&target.referral_id)
        .bind(&establishment_id)
        .bind(payment_id)
        .bind(&subscription_id)
        .bind(inst.installment_index)
        .bind(plan_months)
        .bind(price_centavos)
        .bind(inst.commission_centavos)
        .bind(target.commission_type.as_str())
        .bind(target.commission_value)
        .bind(&available_on_str)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query(
        "UPDATE affiliate_referrals
         SET status = 'active',
             first_paid_at = COALESCE(first_paid_at, ?),
             updated_at = ?
         WHERE id = ? AND status = 'signed_up'",
    )
    .bind(now)
    .bind(now)
    .bind(&target.referral_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

struct EligibleReferral {
    referral_id: String,
    affiliate_id: String,
    commission_type: CommissionType,
    commission_value: i64,
    hold_days: i64,
}

async fn resolve_eligible_referral(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    now: &str,
) -> AppResult<Option<EligibleReferral>> {
    sqlx::query(
        "UPDATE affiliate_referrals
         SET referred_establishment_id = ?, updated_at = ?
         WHERE referred_establishment_id IS NULL
           AND referred_account_id = (
               SELECT account_id FROM establishment_members
               WHERE establishment_id = ? AND member_role = 'owner'
               ORDER BY created_at ASC LIMIT 1
           )",
    )
    .bind(establishment_id)
    .bind(now)
    .bind(establishment_id)
    .execute(&mut **tx)
    .await?;

    let row = sqlx::query(
        "SELECT r.id AS referral_id, r.affiliate_id, r.referred_account_id, r.status AS ref_status,
                a.account_id AS aff_account_id, a.status AS aff_status,
                a.commission_type, a.commission_value, a.hold_days,
                aff_acc.email AS aff_email, ref_acc.email AS ref_email
         FROM affiliate_referrals r
         INNER JOIN affiliates a ON a.id = r.affiliate_id
         INNER JOIN accounts aff_acc ON aff_acc.id = a.account_id
         INNER JOIN accounts ref_acc ON ref_acc.id = r.referred_account_id
         WHERE r.referred_establishment_id = ?",
    )
    .bind(establishment_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let referral_id: String = row.get("referral_id");
    let affiliate_id: String = row.get("affiliate_id");
    let referred_account_id: String = row.get("referred_account_id");
    let ref_status: String = row.get("ref_status");
    let aff_account_id: String = row.get("aff_account_id");
    let aff_status_str: String = row.get("aff_status");
    let aff_email: String = row.get("aff_email");
    let ref_email: String = row.get("ref_email");

    if ref_status == "disqualified" {
        return Ok(None);
    }

    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM establishment_members
            WHERE establishment_id = ? AND account_id = ?
         )",
    )
    .bind(establishment_id)
    .bind(&aff_account_id)
    .fetch_one(&mut **tx)
    .await?;

    if aff_account_id == referred_account_id
        || is_member
        || canonical_email_identity(&aff_email) == canonical_email_identity(&ref_email)
    {
        sqlx::query(
            "UPDATE affiliate_referrals
             SET status = 'disqualified', disqualified_reason = 'self_referral', updated_at = ?
             WHERE id = ?",
        )
        .bind(now)
        .bind(&referral_id)
        .execute(&mut **tx)
        .await?;
        return Ok(None);
    }

    let aff_status = AffiliateStatus::parse(&aff_status_str).unwrap_or(AffiliateStatus::Revoked);
    if !aff_status.accrues_commissions() {
        return Ok(None);
    }

    let kind_str: String = row.get("commission_type");
    let commission_type =
        CommissionType::parse(&kind_str).unwrap_or(CommissionType::FixedMonthlyCentavos);

    Ok(Some(EligibleReferral {
        referral_id,
        affiliate_id,
        commission_type,
        commission_value: row.get("commission_value"),
        hold_days: row.get("hold_days"),
    }))
}

async fn reverse_payment_commissions(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    payment_id: &str,
    today_str: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE affiliate_commissions
         SET status = 'voided', updated_at = ?
         WHERE environment = ? AND payment_id = ? AND entry_kind = 'commission'
           AND status IN ('pending', 'available') AND payout_id IS NULL",
    )
    .bind(now)
    .bind(env)
    .bind(payment_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE affiliate_commissions
         SET status = 'available', updated_at = ?
         WHERE environment = ? AND payment_id = ? AND entry_kind = 'clawback'
           AND status = 'voided' AND payout_id IS NULL",
    )
    .bind(now)
    .bind(env)
    .bind(payment_id)
    .execute(&mut **tx)
    .await?;

    let paid_rows = sqlx::query(
        "SELECT affiliate_id, referral_id, establishment_id, subscription_id,
                installment_index, billing_cycle_months, gross_payment_centavos,
                commission_centavos, snapshot_commission_type, snapshot_commission_value
         FROM affiliate_commissions
         WHERE environment = ? AND payment_id = ? AND entry_kind = 'commission' AND status = 'paid'",
    )
    .bind(env)
    .bind(payment_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in paid_rows {
        let amount: i64 = row.get("commission_centavos");
        sqlx::query(
            "INSERT INTO affiliate_commissions (
                id, environment, affiliate_id, referral_id, establishment_id,
                payment_id, subscription_id, entry_kind, installment_index,
                billing_cycle_months, gross_payment_centavos, commission_centavos,
                snapshot_commission_type, snapshot_commission_value, status,
                available_on, payout_id, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, 'clawback', ?, ?, ?, ?, ?, ?, 'available', ?, NULL, ?, ?)
             ON CONFLICT(environment, payment_id, installment_index, entry_kind) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(env)
        .bind(row.get::<String, _>("affiliate_id"))
        .bind(row.get::<String, _>("referral_id"))
        .bind(row.get::<String, _>("establishment_id"))
        .bind(payment_id)
        .bind(row.get::<String, _>("subscription_id"))
        .bind(row.get::<i64, _>("installment_index"))
        .bind(row.get::<i64, _>("billing_cycle_months"))
        .bind(row.get::<i64, _>("gross_payment_centavos"))
        .bind(-amount)
        .bind(row.get::<String, _>("snapshot_commission_type"))
        .bind(row.get::<i64, _>("snapshot_commission_value"))
        .bind(today_str)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn restore_voided_payment_commissions(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    payment_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE affiliate_commissions
         SET status = 'pending', updated_at = ?
         WHERE environment = ? AND payment_id = ? AND entry_kind = 'commission'
           AND status = 'voided'
           AND EXISTS (
               SELECT 1 FROM affiliate_referrals r
               INNER JOIN affiliates a ON a.id = r.affiliate_id
               WHERE r.id = affiliate_commissions.referral_id
                 AND r.status <> 'disqualified'
                 AND a.status IN ('active', 'paused')
           )",
    )
    .bind(now)
    .bind(env)
    .bind(payment_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE affiliate_commissions
         SET status = 'voided', updated_at = ?
         WHERE environment = ? AND payment_id = ? AND entry_kind = 'clawback'
           AND status = 'available' AND payout_id IS NULL",
    )
    .bind(now)
    .bind(env)
    .bind(payment_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
