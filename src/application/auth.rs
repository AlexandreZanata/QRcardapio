use chrono::{Duration, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::account::{Account, AccountRole, Email, validate_password};
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{
    SessionRecord, delete_session_by_hash, find_account_by_email, find_session_with_account,
    insert_account, insert_reset_token, insert_session, prune_expired_sessions, redeem_reset_token,
    touch_session, update_account_locale,
};
use crate::infrastructure::security::{PasswordService, generate_token_32, sha256_hex};

pub const SESSION_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct AuthSuccess {
    pub account: Account,
    pub raw_session_token: String,
    pub csrf_token: String,
}

pub async fn register_owner(
    pool: &SqlitePool,
    passwords: &PasswordService,
    email_raw: &str,
    password: &str,
    password_confirm: &str,
    locale: Locale,
) -> AppResult<AuthSuccess> {
    let email = Email::parse(email_raw)?;
    validate_password(password)?;
    if password != password_confirm {
        return Err(AppError::Validation("error.password_mismatch"));
    }

    if find_account_by_email(pool, email.as_str()).await?.is_some() {
        return Err(AppError::Conflict("error.email_exists"));
    }

    let password_hash = passwords.hash_password(password).await?;
    let now = Utc::now().to_rfc3339();
    let account = Account {
        id: Uuid::now_v7().to_string(),
        email: email.as_str().to_string(),
        password_hash,
        role: AccountRole::Owner,
        preferred_locale: locale,
        created_at: now.clone(),
        updated_at: now,
    };

    if let Err(err) = insert_account(pool, &account).await {
        if is_unique_violation(&err) {
            return Err(AppError::Conflict("error.email_exists"));
        }
        return Err(err);
    }

    issue_session(pool, account).await
}

pub async fn login(
    pool: &SqlitePool,
    passwords: &PasswordService,
    email_raw: &str,
    password: &str,
) -> AppResult<AuthSuccess> {
    let normalized_email = Email::parse(email_raw).ok();
    let account = match &normalized_email {
        Some(email) => find_account_by_email(pool, email.as_str()).await?,
        None => None,
    };

    let stored_hash = account
        .as_ref()
        .map(|a| a.password_hash.as_str())
        .filter(|hash| !hash.is_empty());
    let valid = passwords.verify_password(password, stored_hash).await?;

    let Some(account) = account else {
        return Err(AppError::Unauthorized("error.invalid_credentials"));
    };
    if !valid {
        return Err(AppError::Unauthorized("error.invalid_credentials"));
    }

    // Rotate session token on every successful authentication.
    issue_session(pool, account).await
}

pub async fn issue_session(pool: &SqlitePool, account: Account) -> AppResult<AuthSuccess> {
    let raw_token = generate_token_32()?;
    let token_hash = sha256_hex(&raw_token);
    let csrf_token = generate_token_32()?;
    let now_dt = Utc::now();
    let expires_dt = now_dt + Duration::days(SESSION_TTL_DAYS);

    insert_session(
        pool,
        &Uuid::now_v7().to_string(),
        &account.id,
        &token_hash,
        &csrf_token,
        &expires_dt.to_rfc3339(),
        &now_dt.to_rfc3339(),
    )
    .await?;

    Ok(AuthSuccess {
        account,
        raw_session_token: raw_token,
        csrf_token,
    })
}

pub async fn authenticate_session(
    pool: &SqlitePool,
    raw_token: &str,
) -> AppResult<Option<(SessionRecord, Account)>> {
    if raw_token.is_empty() || raw_token.len() > 128 {
        return Ok(None);
    }
    let token_hash = sha256_hex(raw_token);
    let now_dt = Utc::now();
    let now_iso = now_dt.to_rfc3339();

    let Some((sess, acc)) = find_session_with_account(pool, &token_hash, &now_iso).await? else {
        return Ok(None);
    };

    // Refresh sliding session timestamp at most once every 15 minutes to avoid DB writes on every GET.
    if let Ok(last_seen) = chrono::DateTime::parse_from_rfc3339(&sess.last_seen_at)
        && now_dt.signed_duration_since(last_seen.with_timezone(&Utc)) > Duration::minutes(15)
    {
        let new_exp = (now_dt + Duration::days(SESSION_TTL_DAYS)).to_rfc3339();
        let _ = touch_session(pool, &sess.id, &new_exp, &now_iso).await;
    }

    Ok(Some((sess, acc)))
}

pub async fn logout(pool: &SqlitePool, raw_token: &str) -> AppResult<()> {
    if raw_token.is_empty() {
        return Ok(());
    }
    let token_hash = sha256_hex(raw_token);
    delete_session_by_hash(pool, &token_hash).await
}

pub async fn set_account_locale(
    pool: &SqlitePool,
    account_id: &str,
    locale: Locale,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    update_account_locale(pool, account_id, locale, &now).await
}

pub async fn create_password_reset_token(pool: &SqlitePool, email_raw: &str) -> AppResult<String> {
    let email = Email::parse(email_raw)?;
    let Some(account) = find_account_by_email(pool, email.as_str()).await? else {
        return Err(AppError::NotFound("error.not_found"));
    };

    let raw_token = generate_token_32()?;
    let token_hash = sha256_hex(&raw_token);
    let now_dt = Utc::now();
    let expires_dt = now_dt + Duration::hours(1);

    insert_reset_token(
        pool,
        &Uuid::now_v7().to_string(),
        &account.id,
        &token_hash,
        &expires_dt.to_rfc3339(),
        &now_dt.to_rfc3339(),
    )
    .await?;

    Ok(raw_token)
}

pub async fn redeem_password_reset(
    pool: &SqlitePool,
    passwords: &PasswordService,
    raw_token: &str,
    new_password: &str,
    confirm_password: &str,
) -> AppResult<()> {
    let trimmed = raw_token.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("error.invalid_reset_token"));
    }
    validate_password(new_password)?;
    if new_password != confirm_password {
        return Err(AppError::Validation("error.password_mismatch"));
    }

    let token_hash = sha256_hex(trimmed);
    let new_hash = passwords.hash_password(new_password).await?;
    let now = Utc::now().to_rfc3339();
    redeem_reset_token(pool, &token_hash, &new_hash, &now).await
}

pub async fn cleanup_expired_sessions(pool: &SqlitePool) -> AppResult<u64> {
    let now = Utc::now().to_rfc3339();
    prune_expired_sessions(pool, &now).await
}

pub fn is_unique_violation(err: &AppError) -> bool {
    if let AppError::Database(sqlx::Error::Database(db_err)) = err {
        return db_err.is_unique_violation();
    }
    false
}
