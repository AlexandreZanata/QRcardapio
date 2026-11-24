use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::auth::is_unique_violation;
use crate::domain::account::Email;
use crate::domain::affiliate::{
    Affiliate, AffiliateCode, AffiliateProgramSettings, AffiliateStatus, CommissionType,
    MAX_NOTES_LEN, parse_commission_input, validate_cookie_ttl_days, validate_hold_days,
    validate_optional_text, validate_pix_key,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{affiliates as queries, find_account_by_email};

pub use super::affiliate_operations::*;

pub struct UpdateProgramSettingsInput<'a> {
    pub self_signup_enabled: bool,
    pub default_commission_type: &'a str,
    pub default_commission_value: &'a str,
    pub default_hold_days: i64,
    pub cookie_ttl_days: i64,
}

pub struct AdminEnrollAffiliateInput<'a> {
    pub email: &'a str,
    pub code: &'a str,
    pub status: &'a str,
    pub commission_type: &'a str,
    pub commission_value: &'a str,
    pub hold_days: i64,
    pub pix_key: &'a str,
    pub notes: &'a str,
}

pub struct AdminUpdateAffiliateInput<'a> {
    pub code: &'a str,
    pub status: &'a str,
    pub commission_type: &'a str,
    pub commission_value: &'a str,
    pub hold_days: i64,
    pub pix_key: &'a str,
    pub notes: &'a str,
}

pub struct SelfActivateAffiliateInput<'a> {
    pub desired_code: &'a str,
    pub pix_key: &'a str,
}

pub async fn get_program_settings(pool: &SqlitePool) -> AppResult<AffiliateProgramSettings> {
    queries::get_program_settings(pool).await
}

pub async fn update_program_settings(
    pool: &SqlitePool,
    input: UpdateProgramSettingsInput<'_>,
) -> AppResult<AffiliateProgramSettings> {
    let default_commission_type = CommissionType::parse(input.default_commission_type)?;
    let default_commission_value =
        parse_commission_input(default_commission_type, input.default_commission_value)?;
    let default_hold_days = validate_hold_days(input.default_hold_days)?;
    let cookie_ttl_days = validate_cookie_ttl_days(input.cookie_ttl_days)?;
    let settings = AffiliateProgramSettings {
        self_signup_enabled: input.self_signup_enabled,
        default_commission_type,
        default_commission_value,
        default_hold_days,
        cookie_ttl_days,
        updated_at: Utc::now().to_rfc3339(),
    };
    queries::update_program_settings(pool, &settings).await?;
    Ok(settings)
}

/// Allows any authenticated account to activate their own affiliate profile using
/// the admin-configured global defaults when open self-signup is enabled.
pub async fn self_activate_affiliate(
    pool: &SqlitePool,
    account_id: &str,
    account_email: &str,
    input: SelfActivateAffiliateInput<'_>,
) -> AppResult<Affiliate> {
    if let Some(existing) = queries::find_affiliate_by_account(pool, account_id).await? {
        return Ok(existing);
    }
    let settings = queries::get_program_settings(pool).await?;
    if !settings.self_signup_enabled {
        return Err(AppError::Forbidden("error.affiliate_signup_closed"));
    }
    let pix_key = validate_pix_key(input.pix_key)?;
    let custom_code = if input.desired_code.trim().is_empty() {
        None
    } else {
        Some(AffiliateCode::parse(input.desired_code)?)
    };

    create_affiliate_record(
        pool,
        account_id,
        account_email,
        custom_code,
        AffiliateStatus::Active,
        settings.default_commission_type,
        settings.default_commission_value,
        settings.default_hold_days,
        pix_key,
        String::new(),
    )
    .await
}

/// Allows an administrator to enroll any existing account (even when self-signup is off)
/// with custom monthly quota, hold days, status, and optional custom referral code.
pub async fn admin_enroll_affiliate(
    pool: &SqlitePool,
    input: AdminEnrollAffiliateInput<'_>,
) -> AppResult<Affiliate> {
    let email = Email::parse(input.email)?;
    let Some(account) = find_account_by_email(pool, email.as_str()).await? else {
        return Err(AppError::NotFound("error.affiliate_account_not_found"));
    };
    if queries::find_affiliate_by_account(pool, &account.id)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("error.affiliate_already_enrolled"));
    }

    let status = AffiliateStatus::parse(input.status)?;
    let commission_type = CommissionType::parse(input.commission_type)?;
    let commission_value = parse_commission_input(commission_type, input.commission_value)?;
    let hold_days = validate_hold_days(input.hold_days)?;
    let pix_key = validate_pix_key(input.pix_key)?;
    let notes = validate_optional_text(input.notes, MAX_NOTES_LEN);
    let custom_code = if input.code.trim().is_empty() {
        None
    } else {
        Some(AffiliateCode::parse(input.code)?)
    };

    create_affiliate_record(
        pool,
        &account.id,
        &account.email,
        custom_code,
        status,
        commission_type,
        commission_value,
        hold_days,
        pix_key,
        notes,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn create_affiliate_record(
    pool: &SqlitePool,
    account_id: &str,
    account_email: &str,
    custom_code: Option<AffiliateCode>,
    status: AffiliateStatus,
    commission_type: CommissionType,
    commission_value: i64,
    hold_days: i64,
    pix_key: String,
    notes: String,
) -> AppResult<Affiliate> {
    let now = Utc::now().to_rfc3339();
    let max_attempts = if custom_code.is_some() { 1 } else { 8 };

    for attempt in 0..max_attempts {
        let code = match &custom_code {
            Some(c) => c.clone(),
            None => {
                let bytes = Uuid::new_v4().into_bytes();
                AffiliateCode::generate_default(
                    account_email,
                    [bytes[0], bytes[1], bytes[2], bytes[3]],
                )
            }
        };
        let aff = Affiliate {
            id: Uuid::now_v7().to_string(),
            account_id: account_id.to_owned(),
            code: code.as_str().to_owned(),
            status,
            commission_type,
            commission_value,
            hold_days,
            pix_key: pix_key.clone(),
            notes: notes.clone(),
            click_count: 0,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        match queries::insert_affiliate(pool, &aff).await {
            Ok(()) => return Ok(aff),
            Err(err) if is_unique_violation(&err) => {
                if queries::find_affiliate_by_account(pool, account_id)
                    .await?
                    .is_some()
                {
                    return Err(AppError::Conflict("error.affiliate_already_enrolled"));
                }
                if custom_code.is_some() || attempt + 1 == max_attempts {
                    return Err(AppError::Conflict("error.affiliate_code_taken"));
                }
            }
            Err(err) => return Err(err),
        }
    }
    Err(AppError::Conflict("error.affiliate_code_taken"))
}

pub async fn admin_update_affiliate(
    pool: &SqlitePool,
    affiliate_id: &str,
    input: AdminUpdateAffiliateInput<'_>,
) -> AppResult<()> {
    let code = AffiliateCode::parse(input.code)?;
    let status = AffiliateStatus::parse(input.status)?;
    let commission_type = CommissionType::parse(input.commission_type)?;
    let commission_value = parse_commission_input(commission_type, input.commission_value)?;
    let hold_days = validate_hold_days(input.hold_days)?;
    let pix_key = validate_pix_key(input.pix_key)?;
    let notes = validate_optional_text(input.notes, MAX_NOTES_LEN);
    let now = Utc::now().to_rfc3339();

    let updated = match queries::update_affiliate_admin(
        pool,
        affiliate_id,
        code.as_str(),
        status,
        commission_type,
        commission_value,
        hold_days,
        &pix_key,
        &notes,
        &now,
    )
    .await
    {
        Ok(found) => found,
        Err(err) if is_unique_violation(&err) => {
            return Err(AppError::Conflict("error.affiliate_code_taken"));
        }
        Err(err) => return Err(err),
    };
    if !updated {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

pub async fn update_own_pix_key(
    pool: &SqlitePool,
    account_id: &str,
    raw_pix_key: &str,
) -> AppResult<()> {
    let Some(aff) = queries::find_affiliate_by_account(pool, account_id).await? else {
        return Err(AppError::NotFound("error.not_found"));
    };
    let pix_key = validate_pix_key(raw_pix_key)?;
    let now = Utc::now().to_rfc3339();
    queries::update_affiliate_pix(pool, &aff.id, &pix_key, &now).await
}
