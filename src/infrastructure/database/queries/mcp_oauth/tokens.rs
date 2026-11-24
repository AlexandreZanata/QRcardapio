use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::infrastructure::database::begin_immediate;

#[derive(sqlx::FromRow)]
pub struct Grant {
    pub id: String,
    pub account_id: String,
    pub establishment_id: String,
    pub scope: String,
    pub preferred_locale: String,
}

pub struct CredentialRotation<'a> {
    pub access_hash: &'a str,
    pub refresh_hash: &'a str,
    pub access_expiry: i64,
}

pub struct CodeExchange<'a> {
    pub hash: &'a str,
    pub client: &'a str,
    pub redirect: &'a str,
    pub challenge: &'a str,
    pub resource: &'a str,
}

pub async fn exchange_code(
    pool: &SqlitePool,
    code: CodeExchange<'_>,
    credentials: CredentialRotation<'_>,
    grant_id: &str,
    now: i64,
    refresh_expiry: i64,
) -> AppResult<Option<String>> {
    #[derive(sqlx::FromRow)]
    struct CodeOwner {
        account_id: String,
        establishment_id: String,
        scope: String,
    }
    let mut tx = begin_immediate(pool).await?;
    let owner: Option<CodeOwner> = sqlx::query_as(
        "DELETE FROM mcp_codes WHERE code_hash = ? AND client_id = ? AND redirect_uri = ?
            AND challenge = ? AND resource = ? AND expires_at > ?
            AND EXISTS (SELECT 1 FROM establishment_members m WHERE m.account_id = mcp_codes.account_id AND m.establishment_id = mcp_codes.establishment_id AND m.member_role = 'owner')
         RETURNING account_id, establishment_id, scope",
    ).bind(code.hash).bind(code.client).bind(code.redirect).bind(code.challenge).bind(code.resource).bind(now)
        .fetch_optional(&mut *tx).await?;
    let Some(owner) = owner else {
        return Ok(None);
    };
    sqlx::query(
        "INSERT INTO mcp_grants (id, establishment_id, account_id, client_id, resource, scope, access_hash,
            refresh_hash, access_expires_at, refresh_expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', ?, 'unixepoch'))
         ON CONFLICT (establishment_id, account_id, client_id) DO UPDATE SET id = excluded.id, resource = excluded.resource,
            scope = excluded.scope, access_hash = excluded.access_hash, refresh_hash = excluded.refresh_hash,
            access_expires_at = excluded.access_expires_at, refresh_expires_at = excluded.refresh_expires_at, created_at = excluded.created_at",
    ).bind(grant_id).bind(owner.establishment_id).bind(owner.account_id).bind(code.client).bind(code.resource)
        .bind(&owner.scope).bind(credentials.access_hash).bind(credentials.refresh_hash).bind(credentials.access_expiry)
        .bind(refresh_expiry).bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(owner.scope))
}

pub async fn rotate_refresh(
    pool: &SqlitePool,
    old_hash: &str,
    client: &str,
    resource: &str,
    scope: Option<&str>,
    credentials: CredentialRotation<'_>,
    now: i64,
) -> AppResult<Option<(String, i64)>> {
    Ok(sqlx::query_as(
        "UPDATE mcp_grants SET access_hash = ?, refresh_hash = ?, access_expires_at = MIN(?, refresh_expires_at),
            scope = COALESCE(?, scope)
         WHERE refresh_hash = ? AND client_id = ? AND resource = ? AND refresh_expires_at > ?
           AND (? IS NULL OR ? = scope OR ? = 'menu.read')
           AND EXISTS (SELECT 1 FROM establishment_members m WHERE m.account_id = mcp_grants.account_id
               AND m.establishment_id = mcp_grants.establishment_id AND m.member_role = 'owner')
         RETURNING scope, access_expires_at",
    ).bind(credentials.access_hash).bind(credentials.refresh_hash).bind(credentials.access_expiry).bind(scope)
        .bind(old_hash).bind(client).bind(resource).bind(now).bind(scope).bind(scope).bind(scope)
        .fetch_optional(pool).await?)
}

pub async fn authenticate(
    pool: &SqlitePool,
    hash: &str,
    resource: &str,
    now: i64,
) -> AppResult<Option<Grant>> {
    Ok(sqlx::query_as(
        "SELECT g.id, g.account_id, g.establishment_id, g.scope, a.preferred_locale FROM mcp_grants g
         JOIN establishment_members m ON m.account_id = g.account_id AND m.establishment_id = g.establishment_id
         JOIN accounts a ON a.id = g.account_id
         WHERE g.access_hash = ? AND g.resource = ? AND g.access_expires_at > ? AND m.member_role = 'owner'",
    ).bind(hash).bind(resource).bind(now).fetch_optional(pool).await?)
}

pub async fn has_active_owner_write_grant(
    pool: &SqlitePool,
    grant: &Grant,
    now: i64,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mcp_grants g
            JOIN establishment_members m
              ON m.establishment_id = g.establishment_id AND m.account_id = g.account_id
         WHERE g.id = ? AND g.account_id = ? AND g.establishment_id = ? AND g.access_expires_at > ?
           AND (' ' || g.scope || ' ') LIKE '% menu.write %' AND m.member_role = 'owner')",
    )
    .bind(&grant.id)
    .bind(&grant.account_id)
    .bind(&grant.establishment_id)
    .bind(now)
    .fetch_one(pool)
    .await?)
}
