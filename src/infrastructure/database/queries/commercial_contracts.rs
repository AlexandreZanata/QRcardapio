use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::domain_events;
use crate::domain::commercial::Quote;
use crate::domain::events::{EventType, NewEvent};
use crate::error::AppResult;

pub const ORIGIN_CHECKOUT: &str = "checkout";

/// Persists the immutable snapshot of a quote. Must run in the transaction that creates the
/// matching `billing_checkouts` row so the gateway amount and the contract never diverge.
pub async fn insert_contract(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    env: &str,
    establishment_id: &str,
    billing_reference: &str,
    quote: &Quote,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO commercial_contracts (
            id, environment, establishment_id, billing_reference, origin, cycle_code, cycle_months,
            cycle_discount_bps, currency, list_total_centavos, total_centavos,
            affiliate_eligible_centavos, activated_at, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, datetime('now'))",
    )
    .bind(id)
    .bind(env)
    .bind(establishment_id)
    .bind(billing_reference)
    .bind(ORIGIN_CHECKOUT)
    .bind(quote.cycle.key())
    .bind(quote.cycle.months())
    .bind(quote.lines.first().map_or(0, |line| line.discount_bps))
    .bind(&quote.currency)
    .bind(quote.list_total_centavos)
    .bind(quote.total_centavos)
    .bind(quote.affiliate_eligible_centavos)
    .execute(&mut **tx)
    .await?;
    for (index, line) in quote.lines.iter().enumerate() {
        let line_no = i64::try_from(index + 1).expect("line number fits in i64");
        sqlx::query(
            "INSERT INTO commercial_contract_items (
                contract_id, line_no, offer_kind, offer_code, offer_version, monthly_price_centavos,
                list_price_centavos, discount_bps, price_centavos, affiliate_eligible
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(line_no)
        .bind(line.kind.as_str())
        .bind(&line.code)
        .bind(i64::from(line.version))
        .bind(line.monthly_price_centavos)
        .bind(line.list_price_centavos)
        .bind(line.discount_bps)
        .bind(line.price_centavos)
        .bind(line.affiliate_eligible)
        .execute(&mut **tx)
        .await?;
        for grant in &line.grants {
            sqlx::query(
                "INSERT INTO commercial_contract_capabilities (contract_id, line_no, offer_code, capability)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(line_no)
            .bind(&grant.offer_code)
            .bind(grant.capability.as_str())
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Marks contracts of a gateway subscription as activated on their first valid confirmed
/// payment and appends `SubscriptionActivated` once per contract (deduplicated).
pub async fn activate_for_subscription(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    subscription_id: &str,
    correlation_id: Option<&str>,
) -> AppResult<()> {
    let activated = sqlx::query(
        "UPDATE commercial_contracts SET activated_at = datetime('now')
         WHERE activated_at IS NULL AND environment = ?
           AND billing_reference IN (
               SELECT reference FROM billing_checkouts WHERE environment = ? AND subscription_id = ?
           )
           AND EXISTS (
               SELECT 1 FROM commercial_entitlement_periods v WHERE v.contract_id = commercial_contracts.id
           )
         RETURNING id, establishment_id, cycle_code, currency, total_centavos",
    )
    .bind(env)
    .bind(env)
    .bind(subscription_id)
    .fetch_all(&mut **tx)
    .await?;
    for row in activated {
        let contract_id: String = row.get("id");
        let establishment_id: String = row.get("establishment_id");
        let offers = sqlx::query(
            "SELECT offer_kind, offer_code, offer_version FROM commercial_contract_items
             WHERE contract_id = ? ORDER BY line_no",
        )
        .bind(&contract_id)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "kind": item.get::<String, _>("offer_kind"),
                "code": item.get::<String, _>("offer_code"),
                "version": item.get::<i64, _>("offer_version"),
            })
        })
        .collect::<Vec<_>>();
        let covered_from: String = sqlx::query_scalar(
            "SELECT MIN(starts_on) FROM commercial_entitlement_periods WHERE contract_id = ?",
        )
        .bind(&contract_id)
        .fetch_one(&mut **tx)
        .await?;
        let payload = serde_json::json!({
            "contract_id": contract_id,
            "environment": env,
            "cycle": row.get::<String, _>("cycle_code"),
            "currency": row.get::<String, _>("currency"),
            "total_centavos": row.get::<i64, _>("total_centavos"),
            "offers": offers,
            "covered_from": covered_from,
        });
        let event = NewEvent::new(
            EventType::SubscriptionActivated,
            &establishment_id,
            &contract_id,
            payload,
        )
        .deduplicated(format!("SubscriptionActivated:{contract_id}"))
        .correlated(correlation_id.map(str::to_owned));
        domain_events::append(tx, &event).await?;
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContractLine {
    pub contract_id: String,
    pub offer_kind: String,
    pub offer_code: String,
    pub offer_version: i64,
    pub price_centavos: i64,
}

/// Lines of every contract of an establishment in one environment, newest contract first.
pub async fn lines_for_establishment(
    pool: &SqlitePool,
    env: &str,
    establishment_id: &str,
) -> AppResult<Vec<ContractLine>> {
    Ok(sqlx::query_as(
        "SELECT i.contract_id, i.offer_kind, i.offer_code, i.offer_version, i.price_centavos
         FROM commercial_contract_items i
         JOIN commercial_contracts k ON k.id = i.contract_id
         WHERE k.environment = ? AND k.establishment_id = ?
         ORDER BY k.created_at DESC, i.line_no ASC",
    )
    .bind(env)
    .bind(establishment_id)
    .fetch_all(pool)
    .await?)
}
