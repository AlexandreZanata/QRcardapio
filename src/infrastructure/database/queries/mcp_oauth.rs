mod tokens;

pub use tokens::*;

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

#[derive(sqlx::FromRow)]
pub struct Authorization {
    pub client_id: String,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub challenge: String,
    pub resource: String,
    pub scope: String,
    pub expires_at: i64,
}

pub async fn save_authorization(
    pool: &SqlitePool,
    id: &str,
    browser: &str,
    flow: &Authorization,
) -> AppResult<()> {
    sqlx::query("INSERT INTO mcp_authorizations (id, browser_hash, client_id, redirect_uri, state, challenge, resource, scope, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(id).bind(browser).bind(&flow.client_id).bind(&flow.redirect_uri).bind(&flow.state)
        .bind(&flow.challenge).bind(&flow.resource).bind(&flow.scope).bind(flow.expires_at).execute(pool).await?;
    Ok(())
}

pub async fn load_authorization(
    pool: &SqlitePool,
    id: &str,
    browser: &str,
    now: i64,
) -> AppResult<Option<Authorization>> {
    Ok(sqlx::query_as("SELECT client_id, redirect_uri, state, challenge, resource, scope, expires_at FROM mcp_authorizations WHERE id = ? AND browser_hash = ? AND expires_at > ?")
        .bind(id).bind(browser).bind(now).fetch_optional(pool).await?)
}

pub async fn finish_authorization(
    pool: &SqlitePool,
    id: &str,
    browser: &str,
    owner: &Owner<'_>,
    code_hash: Option<&str>,
    now: i64,
) -> AppResult<Option<Authorization>> {
    let mut tx = begin_immediate(pool).await?;
    let flow: Option<Authorization> = sqlx::query_as(
        "DELETE FROM mcp_authorizations WHERE id = ? AND browser_hash = ? AND expires_at > ?
         RETURNING client_id, redirect_uri, state, challenge, resource, scope, expires_at",
    )
    .bind(id)
    .bind(browser)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    if let (Some(flow), Some(hash)) = (flow.as_ref(), code_hash) {
        let result = insert_code(&mut tx, owner, flow, hash, now + 120).await?;
        if !result {
            return Ok(None);
        }
    }
    tx.commit().await?;
    Ok(flow)
}

async fn insert_code(
    tx: &mut Transaction<'_, Sqlite>,
    owner: &Owner<'_>,
    flow: &Authorization,
    hash: &str,
    expires: i64,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT INTO mcp_codes (code_hash, establishment_id, account_id, client_id, redirect_uri, challenge, resource, scope, expires_at)
         SELECT ?, establishment_id, account_id, ?, ?, ?, ?, ?, ? FROM establishment_members
         WHERE account_id = ? AND establishment_id = ? AND member_role = 'owner'",
    ).bind(hash).bind(&flow.client_id).bind(&flow.redirect_uri).bind(&flow.challenge)
        .bind(&flow.resource).bind(&flow.scope).bind(expires).bind(owner.account).bind(owner.establishment).execute(&mut **tx).await?;
    Ok(result.rows_affected() == 1)
}

pub struct Owner<'a> {
    pub account: &'a str,
    pub establishment: &'a str,
}

#[derive(sqlx::FromRow)]
pub struct GrantView {
    pub id: String,
    pub client_id: String,
    pub scope: String,
    pub created_at: String,
}

pub async fn list_grants(
    pool: &SqlitePool,
    owner: &Owner<'_>,
    resource: &str,
    now: i64,
) -> AppResult<Vec<GrantView>> {
    Ok(sqlx::query_as("SELECT id, client_id, scope, created_at FROM mcp_grants WHERE account_id = ? AND establishment_id = ? AND resource = ? AND refresh_expires_at > ? ORDER BY created_at DESC")
        .bind(owner.account).bind(owner.establishment).bind(resource).bind(now).fetch_all(pool).await?)
}

pub async fn revoke_grant(pool: &SqlitePool, owner: &Owner<'_>, grant: &str) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let client: Option<String> = sqlx::query_scalar(
        "DELETE FROM mcp_grants WHERE id = ? AND account_id = ? AND establishment_id = ? RETURNING client_id",
    ).bind(grant).bind(owner.account).bind(owner.establishment).fetch_optional(&mut *tx).await?;
    if let Some(client) = client {
        sqlx::query(
            "DELETE FROM mcp_codes WHERE client_id = ? AND account_id = ? AND establishment_id = ?",
        )
        .bind(client)
        .bind(owner.account)
        .bind(owner.establishment)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn cleanup(pool: &SqlitePool, now: i64) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    for query in [
        "DELETE FROM mcp_authorizations WHERE expires_at <= ?",
        "DELETE FROM mcp_codes WHERE expires_at <= ?",
        "DELETE FROM mcp_grants WHERE refresh_expires_at <= ?",
        "DELETE FROM mcp_menu_operations WHERE expires_at <= ?",
    ] {
        sqlx::query(query).bind(now).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
