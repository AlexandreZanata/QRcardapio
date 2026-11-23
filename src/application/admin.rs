use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::auth::is_unique_violation;
use crate::domain::account::{Account, AccountRole, Email, validate_password};
use crate::domain::establishment::Establishment;
use crate::domain::publication::PublicationRecord;
use crate::error::{AppError, AppResult};
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{
    AdminStats, admin_recent_establishments, admin_recent_publications, admin_stats,
    find_account_by_email, insert_account,
};
use crate::infrastructure::database::query_sqlite_version;
use crate::infrastructure::security::PasswordService;

pub struct OperationalOverview {
    pub stats: AdminStats,
    pub sqlite_version: String,
    pub establishments: Vec<Establishment>,
    pub publications: Vec<(PublicationRecord, String, String)>,
}

pub async fn bootstrap_admin(
    pool: &SqlitePool,
    passwords: &PasswordService,
    email_raw: &str,
    password: &str,
) -> AppResult<Account> {
    let email = Email::parse(email_raw)?;
    validate_password(password)?;

    if find_account_by_email(pool, email.as_str()).await?.is_some() {
        return Err(AppError::Conflict("error.email_exists"));
    }

    let password_hash = passwords.hash_password(password).await?;
    let now = Utc::now().to_rfc3339();
    let acc = Account {
        id: Uuid::now_v7().to_string(),
        email: email.as_str().to_string(),
        password_hash,
        role: AccountRole::Admin,
        preferred_locale: Locale::PtBr,
        created_at: now.clone(),
        updated_at: now,
    };

    if let Err(err) = insert_account(pool, &acc).await {
        if is_unique_violation(&err) {
            return Err(AppError::Conflict("error.email_exists"));
        }
        return Err(err);
    }

    Ok(acc)
}

pub async fn load_operational_overview(pool: &SqlitePool) -> AppResult<OperationalOverview> {
    let stats = admin_stats(pool).await?;
    let sqlite_version = query_sqlite_version(pool).await?;
    let establishments = admin_recent_establishments(pool, 25).await?;
    let publications = admin_recent_publications(pool, 25).await?;
    Ok(OperationalOverview {
        stats,
        sqlite_version,
        establishments,
        publications,
    })
}
