use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::mappers::row_to_account;
use crate::domain::account::{Account, AccountRole, Email};
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::begin_immediate;

#[derive(sqlx::FromRow)]
pub struct PendingGoogleFlow {
    pub nonce_hash: String,
    pub code_verifier: String,
    pub purpose: String,
    pub plan_key: Option<String>,
    pub mcp_flow_id: Option<String>,
    pub local_session_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn save_flow(
    pool: &SqlitePool,
    state_hash: &str,
    browser_hash: &str,
    flow: &PendingGoogleFlow,
    client_id: &str,
    redirect: &str,
    expiry: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO google_oauth_flows (state_hash, browser_hash, nonce_hash, code_verifier, purpose, plan_key, mcp_flow_id,
            local_session_id, client_id, redirect_uri, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    ).bind(state_hash).bind(browser_hash).bind(&flow.nonce_hash).bind(&flow.code_verifier).bind(&flow.purpose)
        .bind(&flow.plan_key).bind(&flow.mcp_flow_id).bind(&flow.local_session_id).bind(client_id).bind(redirect).bind(expiry).execute(pool).await?;
    Ok(())
}

pub async fn take_flow(
    pool: &SqlitePool,
    state_hash: &str,
    browser_hash: &str,
    client_id: &str,
    redirect: &str,
    now: i64,
) -> AppResult<Option<PendingGoogleFlow>> {
    Ok(sqlx::query_as(
        "DELETE FROM google_oauth_flows WHERE state_hash = ? AND browser_hash = ? AND client_id = ? AND redirect_uri = ? AND expires_at > ?
         RETURNING nonce_hash, code_verifier, purpose, plan_key, mcp_flow_id, local_session_id",
    ).bind(state_hash).bind(browser_hash).bind(client_id).bind(redirect).bind(now).fetch_optional(pool).await?)
}

pub async fn prune_flows(pool: &SqlitePool, now: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM google_oauth_flows WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn resolve_account(
    pool: &SqlitePool,
    subject: &str,
    email: &Email,
    locale: Locale,
) -> AppResult<(Account, bool)> {
    let mut tx = begin_immediate(pool).await?;
    let existing = sqlx::query("SELECT a.* FROM google_identities g JOIN accounts a ON a.id = g.account_id WHERE g.subject = ?")
        .bind(subject).fetch_optional(&mut *tx).await?;
    if let Some(row) = existing {
        tx.commit().await?;
        return Ok((row_to_account(&row), false));
    }
    let now = Utc::now();
    let account = Account {
        id: Uuid::now_v7().to_string(),
        email: email.as_str().to_owned(),
        password_hash: String::new(),
        role: AccountRole::Owner,
        preferred_locale: locale,
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
    };
    let inserted = sqlx::query("INSERT INTO accounts (id, email, password_hash, role, preferred_locale, created_at, updated_at)
        VALUES (?, ?, '', 'owner', ?, ?, ?) ON CONFLICT(email) DO NOTHING")
        .bind(&account.id).bind(&account.email).bind(locale.code()).bind(&account.created_at).bind(&account.updated_at)
        .execute(&mut *tx).await?;
    if inserted.rows_affected() != 1 {
        return Err(AppError::Conflict("google.link_required"));
    }
    sqlx::query("INSERT INTO google_identities (subject, account_id, linked_at) VALUES (?, ?, ?)")
        .bind(subject)
        .bind(&account.id)
        .bind(now.timestamp())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((account, true))
}

pub async fn has_identity(pool: &SqlitePool, account: &str) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM google_identities WHERE account_id = ?)")
            .bind(account)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn link_identity(
    pool: &SqlitePool,
    account: &str,
    session: &str,
    subject: &str,
    email: &Email,
) -> AppResult<()> {
    let now = Utc::now();
    let mut tx = begin_immediate(pool).await?;
    let eligible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sessions s JOIN accounts a ON a.id = s.account_id
         WHERE s.id = ? AND s.account_id = ? AND s.expires_at > ? AND a.email = ?)",
    )
    .bind(session)
    .bind(account)
    .bind(now.to_rfc3339())
    .bind(email.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !eligible {
        return Err(AppError::Forbidden("google.link_mismatch"));
    }
    let owner: Option<String> =
        sqlx::query_scalar("SELECT account_id FROM google_identities WHERE subject = ?")
            .bind(subject)
            .fetch_optional(&mut *tx)
            .await?;
    if owner.as_deref() == Some(account) {
        tx.commit().await?;
        return Ok(());
    }
    let result = sqlx::query("INSERT INTO google_identities (subject, account_id, linked_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
        .bind(subject).bind(account).bind(now.timestamp()).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("google.link_conflict"));
    }
    tx.commit().await?;
    Ok(())
}

pub async fn unlink_identity(pool: &SqlitePool, account: &str) -> AppResult<bool> {
    let deleted = sqlx::query(
        "DELETE FROM google_identities WHERE account_id = ? AND EXISTS (
         SELECT 1 FROM accounts a WHERE a.id = google_identities.account_id AND a.password_hash <> '')",
    ).bind(account).execute(pool).await?;
    Ok(deleted.rows_affected() == 1)
}
