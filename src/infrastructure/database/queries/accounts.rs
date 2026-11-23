use sqlx::SqlitePool;

use super::mappers::row_to_account;
use crate::domain::account::Account;
use crate::error::AppResult;
use crate::i18n::Locale;

pub async fn insert_account(pool: &SqlitePool, acc: &Account) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO accounts (id, email, password_hash, role, preferred_locale, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&acc.id)
    .bind(&acc.email)
    .bind(&acc.password_hash)
    .bind(acc.role.as_str())
    .bind(acc.preferred_locale.code())
    .bind(&acc.created_at)
    .bind(&acc.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_account_by_email(pool: &SqlitePool, email: &str) -> AppResult<Option<Account>> {
    let row = sqlx::query(
        "SELECT id, email, password_hash, role, preferred_locale, created_at, updated_at
         FROM accounts WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_account(&r)))
}

pub async fn find_account_by_id(pool: &SqlitePool, id: &str) -> AppResult<Option<Account>> {
    let row = sqlx::query(
        "SELECT id, email, password_hash, role, preferred_locale, created_at, updated_at
         FROM accounts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_account(&r)))
}

pub async fn update_account_locale(
    pool: &SqlitePool,
    account_id: &str,
    locale: Locale,
    now: &str,
) -> AppResult<()> {
    sqlx::query("UPDATE accounts SET preferred_locale = ?, updated_at = ? WHERE id = ?")
        .bind(locale.code())
        .bind(now)
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_account_password(
    pool: &SqlitePool,
    account_id: &str,
    password_hash: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query("UPDATE accounts SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(password_hash)
        .bind(now)
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}
