use sqlx::{Row, SqlitePool};

use crate::domain::account::{Account, AccountRole};
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::begin_immediate;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub account_id: String,
    pub csrf_token: String,
    pub expires_at: String,
    pub last_seen_at: String,
}

pub async fn insert_session(
    pool: &SqlitePool,
    id: &str,
    account_id: &str,
    token_hash: &str,
    csrf_token: &str,
    expires_at: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, csrf_token, expires_at, created_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(account_id)
    .bind(token_hash)
    .bind(csrf_token)
    .bind(expires_at)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_session_with_account(
    pool: &SqlitePool,
    token_hash: &str,
    now: &str,
) -> AppResult<Option<(SessionRecord, Account)>> {
    let row = sqlx::query(
        "SELECT s.id AS s_id, s.account_id AS s_account_id, s.csrf_token AS s_csrf_token,
                s.expires_at AS s_expires_at, s.last_seen_at AS s_last_seen_at,
                a.id AS a_id, a.email AS a_email, a.password_hash AS a_password_hash,
                a.role AS a_role, a.preferred_locale AS a_locale,
                a.created_at AS a_created_at, a.updated_at AS a_updated_at
         FROM sessions s
         INNER JOIN accounts a ON a.id = s.account_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let sess = SessionRecord {
            id: r.get("s_id"),
            account_id: r.get("s_account_id"),
            csrf_token: r.get("s_csrf_token"),
            expires_at: r.get("s_expires_at"),
            last_seen_at: r.get("s_last_seen_at"),
        };
        let acc = Account {
            id: r.get("a_id"),
            email: r.get("a_email"),
            password_hash: r.get("a_password_hash"),
            role: AccountRole::from_db(r.get::<String, _>("a_role").as_str()),
            preferred_locale: Locale::parse(r.get::<String, _>("a_locale").as_str())
                .unwrap_or(Locale::PtBr),
            created_at: r.get("a_created_at"),
            updated_at: r.get("a_updated_at"),
        };
        (sess, acc)
    }))
}

pub async fn touch_session(
    pool: &SqlitePool,
    session_id: &str,
    new_expires_at: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE id = ?")
        .bind(now)
        .bind(new_expires_at)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_session_by_hash(pool: &SqlitePool, token_hash: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_sessions_for_account(pool: &SqlitePool, account_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE account_id = ?")
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn prune_expired_sessions(pool: &SqlitePool, now: &str) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn insert_reset_token(
    pool: &SqlitePool,
    id: &str,
    account_id: &str,
    token_hash: &str,
    expires_at: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO password_reset_tokens (id, account_id, token_hash, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(account_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn redeem_reset_token(
    pool: &SqlitePool,
    token_hash: &str,
    new_password_hash: &str,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let row = sqlx::query(
        "SELECT id, account_id FROM password_reset_tokens
         WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(r) = row else {
        return Err(AppError::Validation("error.invalid_reset_token"));
    };

    let token_id: String = r.get("id");
    let account_id: String = r.get("account_id");

    sqlx::query("UPDATE password_reset_tokens SET used_at = ? WHERE id = ?")
        .bind(now)
        .bind(&token_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE accounts SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(new_password_hash)
        .bind(now)
        .bind(&account_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM sessions WHERE account_id = ?")
        .bind(&account_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
