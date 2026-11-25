use std::collections::BTreeSet;

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::domain::commercial::{CommercialCatalog, Lifecycle, OfferFingerprint};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::security::sha256_hex;

/// Records the compiled catalog and refuses start-up when it would silently change sold terms:
/// an illegal lifecycle transition, a new offer shipped directly as sellable, a changed sellable
/// version, or a contract that references a version missing from the catalog (ADR-007).
///
/// Rollback tolerance (ADR-010): the registry is written at start-up, before deploy smoke
/// checks. A binary rolled back after a failed deploy may therefore see the reverse of a legal
/// transition or miss an offer the failed release added; both are accepted with a warning so the
/// previous release can start. Contracts referencing unknown versions are always refused.
pub async fn sync_registry(pool: &SqlitePool, catalog: &CommercialCatalog) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    let offers = catalog.fingerprints();
    let known: BTreeSet<(String, String)> = offers
        .iter()
        .map(|o| (o.kind.as_str().to_owned(), o.code.clone()))
        .collect();

    let stored =
        sqlx::query("SELECT offer_kind, offer_code, lifecycle FROM commercial_catalog_offers")
            .fetch_all(&mut *tx)
            .await?;
    let first_sync = stored.is_empty();
    for row in &stored {
        let key: (String, String) = (row.get("offer_kind"), row.get("offer_code"));
        let lifecycle = Lifecycle::parse(row.get::<&str, _>("lifecycle"));
        if !known.contains(&key) && lifecycle.is_some_and(Lifecycle::ever_sellable) {
            tracing::warn!(
                offer_kind = %key.0,
                offer_code = %key.1,
                "sellable offer missing from this binary's catalog (rollback?); contracts are still checked"
            );
        }
    }

    for offer in &offers {
        record_offer(&mut tx, offer, &now, first_sync).await?;
    }

    let referenced = sqlx::query(
        "SELECT DISTINCT offer_kind, offer_code, offer_version FROM commercial_contract_items",
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in referenced {
        let (kind, code, version): (String, String, i64) = (
            row.get("offer_kind"),
            row.get("offer_code"),
            row.get("offer_version"),
        );
        let exists = offers.iter().any(|o| {
            o.kind.as_str() == kind
                && o.code == code
                && o.versions.iter().any(|v| i64::from(v.version) == version)
        });
        if !exists {
            return Err(registry_error(format!(
                "contracts reference {kind}/{code} v{version}, which the catalog no longer defines"
            )));
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn record_offer(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    offer: &OfferFingerprint,
    now: &str,
    first_sync: bool,
) -> AppResult<()> {
    let kind = offer.kind.as_str();
    let previous: Option<String> = sqlx::query_scalar(
        "SELECT lifecycle FROM commercial_catalog_offers WHERE offer_kind = ? AND offer_code = ?",
    )
    .bind(kind)
    .bind(&offer.code)
    .fetch_optional(&mut **tx)
    .await?;
    let previous = previous.as_deref().and_then(Lifecycle::parse);
    match previous {
        None if !first_sync && offer.lifecycle.ever_sellable() => {
            return Err(registry_error(format!(
                "new offer {kind}/{} must first ship as planned or development, not {}",
                offer.code,
                offer.lifecycle.as_str()
            )));
        }
        Some(previous) if previous != offer.lifecycle => {
            if offer.lifecycle.can_transition_to(previous) {
                tracing::warn!(
                    offer = %offer.code,
                    from = previous.as_str(),
                    to = offer.lifecycle.as_str(),
                    "lifecycle reverted to its previous state (rollback)"
                );
            } else if !previous.can_transition_to(offer.lifecycle) {
                return Err(registry_error(format!(
                    "{kind}/{} cannot move from {} to {}",
                    offer.code,
                    previous.as_str(),
                    offer.lifecycle.as_str()
                )));
            }
        }
        _ => {}
    }
    sqlx::query(
        "INSERT INTO commercial_catalog_offers
            (offer_kind, offer_code, lifecycle, current_version, first_recorded_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(offer_kind, offer_code) DO UPDATE SET
            lifecycle = excluded.lifecycle, current_version = excluded.current_version,
            updated_at = excluded.updated_at
         WHERE lifecycle <> excluded.lifecycle OR current_version IS NOT excluded.current_version",
    )
    .bind(kind)
    .bind(&offer.code)
    .bind(offer.lifecycle.as_str())
    .bind(offer.current_version.map(i64::from))
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    let frozen = offer.lifecycle.ever_sellable() || previous.is_some_and(Lifecycle::ever_sellable);
    if !frozen {
        return Ok(());
    }
    for version in &offer.versions {
        let hash = sha256_hex(&version.canonical);
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT content_hash FROM commercial_catalog_versions
             WHERE offer_kind = ? AND offer_code = ? AND version = ?",
        )
        .bind(kind)
        .bind(&offer.code)
        .bind(i64::from(version.version))
        .fetch_optional(&mut **tx)
        .await?;
        match stored {
            Some(existing) if existing != hash => {
                return Err(registry_error(format!(
                    "{kind}/{} v{} changed after it was sold; publish a new version instead",
                    offer.code, version.version
                )));
            }
            Some(_) => {}
            None => {
                sqlx::query(
                    "INSERT INTO commercial_catalog_versions
                        (offer_kind, offer_code, version, content_hash, recorded_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(kind)
                .bind(&offer.code)
                .bind(i64::from(version.version))
                .bind(&hash)
                .bind(now)
                .execute(&mut **tx)
                .await?;
            }
        }
    }
    Ok(())
}

fn registry_error(message: String) -> AppError {
    AppError::Internal(format!("commercial catalog registry: {message}"))
}
