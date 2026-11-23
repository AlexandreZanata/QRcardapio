use sqlx::{Row, SqlitePool};

use super::mappers::row_to_establishment;
use crate::domain::establishment::Establishment;
use crate::domain::publication::PublicationRecord;
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct AdminStats {
    pub account_count: i64,
    pub establishment_count: i64,
    pub published_count: i64,
    pub publication_count: i64,
}

pub async fn admin_stats(pool: &SqlitePool) -> AppResult<AdminStats> {
    let account_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(pool)
        .await?;
    let establishment_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM establishments")
        .fetch_one(pool)
        .await?;
    let published_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM establishments WHERE is_published = 1")
            .fetch_one(pool)
            .await?;
    let publication_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM publications")
        .fetch_one(pool)
        .await?;
    Ok(AdminStats {
        account_count,
        establishment_count,
        published_count,
        publication_count,
    })
}

pub async fn admin_recent_establishments(
    pool: &SqlitePool,
    limit: i64,
) -> AppResult<Vec<Establishment>> {
    let rows = sqlx::query("SELECT * FROM establishments ORDER BY created_at DESC LIMIT ?")
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(row_to_establishment).collect())
}

pub async fn admin_recent_publications(
    pool: &SqlitePool,
    limit: i64,
) -> AppResult<Vec<(PublicationRecord, String, String)>> {
    let rows = sqlx::query(
        "SELECT p.*, e.name AS est_name, e.slug AS est_slug
         FROM publications p
         INNER JOIN establishments e ON e.id = p.establishment_id
         ORDER BY p.published_at DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let rec = PublicationRecord {
                id: r.get("id"),
                establishment_id: r.get("establishment_id"),
                version: r.get("version"),
                status: r.get("status"),
                html_bytes: r.get("html_bytes"),
                asset_count: r.get("asset_count"),
                duration_ms: r.get("duration_ms"),
                error_message: r.get("error_message"),
                published_at: r.get("published_at"),
            };
            let est_name: String = r.get("est_name");
            let est_slug: String = r.get("est_slug");
            (rec, est_name, est_slug)
        })
        .collect())
}
