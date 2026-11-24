//! Platform-operator use cases (CLI): explicit entitlement grants and operational suspension.
//! Every action requires an existing platform admin account, is validated against the
//! catalog lifecycle and appends an audit event in the same transaction.

use chrono::{NaiveDate, Utc};
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::account::AccountRole;
use crate::domain::commercial::{OfferKind, catalog};
use crate::domain::entitlements::{GrantKind, OperationalStatus};
use crate::domain::events::{EventType, NewEvent};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::database::queries::{
    domain_events, entitlements as queries, find_account_by_email,
};

pub struct GrantRequest<'a> {
    pub operator_email: &'a str,
    pub establishment_slug: &'a str,
    pub offer_code: &'a str,
    pub kind: GrantKind,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: &'a str,
}

fn validate_reason(reason: &str) -> AppResult<&str> {
    let reason = reason.trim();
    if (3..=400).contains(&reason.chars().count()) {
        Ok(reason)
    } else {
        Err(AppError::Validation("error.operator_reason_invalid"))
    }
}

async fn operator_id(pool: &SqlitePool, email: &str) -> AppResult<String> {
    let account = find_account_by_email(pool, email)
        .await?
        .filter(|account| account.role == AccountRole::Admin)
        .ok_or(AppError::Forbidden("error.forbidden"))?;
    Ok(account.id)
}

async fn establishment_id(pool: &SqlitePool, slug: &str) -> AppResult<String> {
    sqlx::query_scalar("SELECT id FROM establishments WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))
}

/// Creates an explicit grant of a module or recurring add-on. Returns the grant id.
pub async fn grant(pool: &SqlitePool, request: GrantRequest<'_>) -> AppResult<String> {
    let reason = validate_reason(request.reason)?;
    let operator = operator_id(pool, request.operator_email).await?;
    let establishment = establishment_id(pool, request.establishment_slug).await?;
    let (offer_kind, lifecycle) = match (
        catalog().module(request.offer_code),
        catalog().addon(request.offer_code),
    ) {
        (Some(module), _) => (OfferKind::Module, module.lifecycle),
        (None, Some(addon)) => (OfferKind::Addon, addon.lifecycle),
        (None, None) => return Err(AppError::Validation("error.commercial_offer_invalid")),
    };
    if catalog().offer_capabilities(request.offer_code).is_empty()
        || !request.kind.allowed_for(lifecycle)
    {
        return Err(AppError::Validation("error.commercial_offer_unavailable"));
    }
    let days = (request.ends_on - request.starts_on).num_days();
    if days < 1 || days > request.kind.max_days() {
        return Err(AppError::Validation("error.operator_period_invalid"));
    }
    let id = Uuid::now_v7().to_string();
    let now = Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    let inserted = queries::insert_grant(
        &mut tx,
        &queries::NewGrant {
            id: &id,
            establishment_id: &establishment,
            offer_kind: offer_kind.as_str(),
            offer_code: request.offer_code,
            grant_kind: request.kind.as_str(),
            starts_on: request.starts_on,
            ends_on: request.ends_on,
            reason,
            granted_by: &operator,
            now: &now,
        },
    )
    .await?;
    if !inserted {
        return Err(AppError::Conflict("error.operator_trial_used"));
    }
    let event = NewEvent::new(
        EventType::EntitlementGranted,
        &establishment,
        &id,
        json!({
            "grant_id": id,
            "offer": request.offer_code,
            "grant_kind": request.kind.as_str(),
            "starts_on": request.starts_on.to_string(),
            "ends_on": request.ends_on.to_string(),
        }),
    )
    .deduplicated(format!("EntitlementGranted:{id}"));
    domain_events::append(&mut tx, &event).await?;
    tx.commit().await?;
    Ok(id)
}

/// Revokes an explicit grant. Capabilities still covered by another source stay active.
pub async fn revoke(
    pool: &SqlitePool,
    operator_email: &str,
    establishment_slug: &str,
    grant_id: &str,
) -> AppResult<()> {
    let operator = operator_id(pool, operator_email).await?;
    let establishment = establishment_id(pool, establishment_slug).await?;
    let now = Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    let revoked = queries::revoke_grant(&mut tx, &establishment, grant_id, &operator, &now)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let event = NewEvent::new(
        EventType::EntitlementRevoked,
        &establishment,
        &revoked.id,
        json!({
            "grant_id": revoked.id,
            "offer": revoked.offer_code,
            "grant_kind": revoked.grant_kind,
        }),
    )
    .deduplicated(format!("EntitlementRevoked:{}", revoked.id));
    domain_events::append(&mut tx, &event).await?;
    tx.commit().await?;
    Ok(())
}

/// Sets the operational status. Suspension blocks new protected commercial operations only;
/// published menus, data, unpublishing and billing regularization remain available.
pub async fn set_operational_status(
    pool: &SqlitePool,
    operator_email: &str,
    establishment_slug: &str,
    status: OperationalStatus,
    reason: &str,
) -> AppResult<()> {
    let reason = validate_reason(reason)?;
    let operator = operator_id(pool, operator_email).await?;
    let establishment = establishment_id(pool, establishment_slug).await?;
    let now = Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    let previous = queries::set_operational_status(
        &mut tx,
        &establishment,
        status.as_str(),
        reason,
        &operator,
        &now,
    )
    .await?;
    let previous = OperationalStatus::parse(&previous).unwrap_or(OperationalStatus::Active);
    let event = match (previous, status) {
        (from, to) if from == to => None,
        (_, OperationalStatus::Active) => Some(NewEvent::new(
            EventType::EstablishmentReinstated,
            &establishment,
            &establishment,
            json!({ "previous_status": previous.as_str() }),
        )),
        (_, suspended) => Some(NewEvent::new(
            EventType::EstablishmentSuspended,
            &establishment,
            &establishment,
            json!({ "status": suspended.as_str() }),
        )),
    };
    if let Some(event) = event {
        domain_events::append(&mut tx, &event).await?;
    }
    tx.commit().await?;
    Ok(())
}
