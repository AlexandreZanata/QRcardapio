use chrono::{NaiveDate, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::affiliate::{
    Affiliate, AffiliateCode, AffiliateProgramSettings, MAX_NOTES_LEN, MAX_PAYOUT_REF_LEN,
    canonical_email_identity, validate_optional_text,
};
use crate::domain::billing::billing_now;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    affiliate_ledger::{self, AffiliateBalances, CommissionLedgerRow, PayoutRow, SettledPayout},
    affiliates as queries,
};

pub struct AffiliatePortalData {
    pub settings: AffiliateProgramSettings,
    pub affiliate: Option<Affiliate>,
    pub balances: AffiliateBalances,
    pub referrals: Vec<queries::ReferralViewRow>,
    pub commissions: Vec<CommissionLedgerRow>,
    pub payouts: Vec<PayoutRow>,
}

pub struct AdminAffiliateDetailData {
    pub affiliate: Affiliate,
    pub account_email: String,
    pub balances: AffiliateBalances,
    pub referrals: Vec<queries::ReferralViewRow>,
    pub commissions: Vec<CommissionLedgerRow>,
    pub payouts: Vec<PayoutRow>,
}

/// Validates a referral code on click, increments click stats when not already attributed
/// in the visitor's cookie, and returns `(normalized_code, cookie_ttl_days)` if active.
pub async fn record_referral_click(
    pool: &SqlitePool,
    raw_code: &str,
    existing_cookie_code: Option<&str>,
) -> AppResult<Option<(String, i64)>> {
    let Ok(code) = AffiliateCode::parse(raw_code) else {
        return Ok(None);
    };
    let Some(aff) = queries::find_affiliate_by_code(pool, code.as_str()).await? else {
        return Ok(None);
    };
    if !aff.status.accepts_new_referrals() {
        return Ok(None);
    }
    if existing_cookie_code != Some(aff.code.as_str()) {
        queries::increment_affiliate_clicks(pool, &aff.id).await?;
    }
    let settings = queries::get_program_settings(pool).await?;
    Ok(Some((aff.code, settings.cookie_ttl_days)))
}

/// Attributes a newly created account to an active affiliate (once, immutably).
/// Enforces anti-self-referral checks (same account ID or canonical email identity).
pub async fn attribute_new_account(
    pool: &SqlitePool,
    raw_code: Option<&str>,
    new_account_id: &str,
    new_account_email: &str,
) -> AppResult<bool> {
    let Some(raw_code) = raw_code else {
        return Ok(false);
    };
    let Ok(code) = AffiliateCode::parse(raw_code) else {
        return Ok(false);
    };
    let Some((aff, aff_email)) =
        queries::find_affiliate_with_email_by_code(pool, code.as_str()).await?
    else {
        return Ok(false);
    };
    if !aff.status.accepts_new_referrals() {
        return Ok(false);
    }
    if aff.account_id == new_account_id
        || canonical_email_identity(&aff_email) == canonical_email_identity(new_account_email)
    {
        return Ok(false);
    }
    let now = Utc::now().to_rfc3339();
    queries::insert_referral(
        pool,
        &Uuid::now_v7().to_string(),
        &aff.id,
        new_account_id,
        &now,
    )
    .await
}

pub async fn bind_establishment_referral(
    pool: &SqlitePool,
    account_id: &str,
    establishment_id: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    queries::bind_referral_establishment(pool, account_id, establishment_id, &now).await
}

pub async fn admin_disqualify_referral(
    pool: &SqlitePool,
    affiliate_id: &str,
    referral_id: &str,
    reason_raw: &str,
) -> AppResult<()> {
    let reason = validate_optional_text(reason_raw, 120);
    let reason = if reason.is_empty() {
        "admin_disqualified".to_owned()
    } else {
        reason
    };
    let now = Utc::now().to_rfc3339();
    let changed =
        queries::disqualify_referral(pool, affiliate_id, referral_id, &reason, &now).await?;
    if !changed {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

pub async fn settle_payout(
    pool: &SqlitePool,
    affiliate_id: &str,
    env: &str,
    admin_account_id: &str,
    reference_raw: &str,
    notes_raw: &str,
    as_of: NaiveDate,
) -> AppResult<SettledPayout> {
    let reference = validate_optional_text(reference_raw, MAX_PAYOUT_REF_LEN);
    let notes = validate_optional_text(notes_raw, MAX_NOTES_LEN);
    let now = Utc::now().to_rfc3339();
    affiliate_ledger::settle_affiliate_payout(
        pool,
        affiliate_id,
        env,
        admin_account_id,
        &reference,
        &notes,
        as_of,
        &now,
    )
    .await
}

pub async fn load_affiliate_portal(
    pool: &SqlitePool,
    account_id: &str,
    env: &str,
) -> AppResult<AffiliatePortalData> {
    let today = billing_now().date_naive();
    let settings = queries::get_program_settings(pool).await?;
    let affiliate = queries::find_affiliate_by_account(pool, account_id).await?;
    let Some(aff) = affiliate else {
        return Ok(AffiliatePortalData {
            settings,
            affiliate: None,
            balances: AffiliateBalances::default(),
            referrals: Vec::new(),
            commissions: Vec::new(),
            payouts: Vec::new(),
        });
    };

    let (balances, referrals, commissions, payouts) = tokio::try_join!(
        affiliate_ledger::affiliate_balances(pool, &aff.id, env, today),
        queries::list_referrals_for_affiliate(pool, &aff.id, 100),
        affiliate_ledger::list_commissions_for_affiliate(pool, &aff.id, env, today, 100),
        affiliate_ledger::list_payouts_for_affiliate(pool, &aff.id, env, 50),
    )?;

    Ok(AffiliatePortalData {
        settings,
        affiliate: Some(aff),
        balances,
        referrals,
        commissions,
        payouts,
    })
}

pub async fn load_admin_overview(
    pool: &SqlitePool,
    env: &str,
) -> AppResult<(
    AffiliateProgramSettings,
    queries::AffiliateProgramStats,
    Vec<queries::AffiliateSummaryRow>,
)> {
    let today = billing_now().date_naive();
    tokio::try_join!(
        queries::get_program_settings(pool),
        queries::program_overview_stats(pool, env, today),
        queries::list_affiliates_summary(pool, env, today),
    )
}

pub async fn load_admin_affiliate_detail(
    pool: &SqlitePool,
    affiliate_id: &str,
    env: &str,
) -> AppResult<AdminAffiliateDetailData> {
    let today = billing_now().date_naive();
    let Some((affiliate, account_email)) =
        queries::find_affiliate_with_email_by_id(pool, affiliate_id).await?
    else {
        return Err(AppError::NotFound("error.not_found"));
    };
    let (balances, referrals, commissions, payouts) = tokio::try_join!(
        affiliate_ledger::affiliate_balances(pool, &affiliate.id, env, today),
        queries::list_referrals_for_affiliate(pool, &affiliate.id, 200),
        affiliate_ledger::list_commissions_for_affiliate(pool, &affiliate.id, env, today, 200),
        affiliate_ledger::list_payouts_for_affiliate(pool, &affiliate.id, env, 100),
    )?;
    Ok(AdminAffiliateDetailData {
        affiliate,
        account_email,
        balances,
        referrals,
        commissions,
        payouts,
    })
}
