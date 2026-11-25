//! Reserves Inter cycle charges before contacting the bank and recovers ambiguous submissions.

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Timelike, Utc};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use super::{INTER, environment};
use crate::domain::billing::billing_now;
use crate::domain::billing_cycle::{PAYABLE_DAYS_AFTER_DUE, our_number};
use crate::domain::commercial::Quote;
use crate::domain::payments::PaymentMethod;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::database::queries::{billing as queries, billing_inter, billing_payers};
use crate::infrastructure::inter::{InterClient, NewCharge};

/// First due date: today while there is headroom before Inter's same-day cutoff.
fn first_due_date(now: DateTime<FixedOffset>) -> NaiveDate {
    let today = now.date_naive();
    if now.hour() < 19 {
        today
    } else {
        today + Duration::days(1)
    }
}

/// Opens a Pix/boleto subscription and issues its first charge.
pub async fn start(
    pool: &SqlitePool,
    client: &InterClient,
    establishment: &str,
    quote: &Quote,
) -> AppResult<()> {
    let env = environment(client);
    if billing_payers::find(pool, establishment).await?.is_none() {
        return Err(AppError::Validation("error.payer_required"));
    }
    let reference = Uuid::now_v7().to_string();
    let target = queries::CheckoutTarget {
        provider: INTER,
        method: PaymentMethod::PixBoleto,
    };
    let now = billing_now();
    let mut tx = begin_immediate(pool).await?;
    if !queries::reserve_contract_checkout_in_transaction(
        &mut tx,
        env,
        establishment,
        &reference,
        now.date_naive(),
        quote,
        target,
    )
    .await?
    {
        tx.commit().await?;
        return Ok(());
    }
    if !reserve_cycle(
        &mut tx,
        env,
        &reference,
        establishment,
        0,
        first_due_date(now),
        quote.total_centavos,
    )
    .await?
    {
        return Err(AppError::Internal(
            "Inter first charge reservation already exists".into(),
        ));
    }
    tx.commit().await?;
    submit_reserved(pool, client, &reference, 0).await
}

/// Reserves and issues the charge of one cycle. Safe to call again for the same cycle.
pub async fn issue_cycle(
    pool: &SqlitePool,
    client: &InterClient,
    reference: &str,
    establishment: &str,
    cycle_index: i64,
    due_date: NaiveDate,
    amount_centavos: i64,
) -> AppResult<()> {
    let env = environment(client);
    let mut tx = begin_immediate(pool).await?;
    reserve_cycle(
        &mut tx,
        env,
        reference,
        establishment,
        cycle_index,
        due_date,
        amount_centavos,
    )
    .await?;
    tx.commit().await?;
    submit_reserved(pool, client, reference, cycle_index).await
}

async fn reserve_cycle(
    tx: &mut Transaction<'_, Sqlite>,
    env: &str,
    reference: &str,
    establishment: &str,
    cycle_index: i64,
    due_date: NaiveDate,
    amount_centavos: i64,
) -> AppResult<bool> {
    let number = our_number(reference, cycle_index);
    let now = Utc::now().to_rfc3339();
    billing_inter::reserve(
        tx,
        env,
        &billing_inter::NewCharge {
            our_number: &number,
            billing_reference: reference,
            establishment_id: establishment,
            cycle_index,
            due_date,
            amount_centavos,
            now: &now,
        },
    )
    .await
}

async fn submit_reserved(
    pool: &SqlitePool,
    client: &InterClient,
    reference: &str,
    cycle_index: i64,
) -> AppResult<()> {
    let charge = billing_inter::find_by_cycle(pool, reference, cycle_index)
        .await?
        .ok_or(AppError::Internal(
            "reserved Inter charge disappeared".into(),
        ))?;
    if charge.request_code.is_some() {
        return Ok(());
    }
    submit(pool, client, &charge).await
}

pub(super) async fn submit(
    pool: &SqlitePool,
    client: &InterClient,
    charge: &billing_inter::InterCharge,
) -> AppResult<()> {
    let env = environment(client);
    let payer = billing_payers::find(pool, &charge.establishment_id)
        .await?
        .ok_or(AppError::Validation("error.payer_required"))?;
    let message = format!(
        "QR Cardapio - assinatura - ciclo {}",
        charge.cycle_index + 1
    );
    let created = client
        .create_charge(&NewCharge {
            our_number: &charge.our_number,
            amount_centavos: charge.amount_centavos,
            due_date: charge.due_date,
            payable_days_after_due: PAYABLE_DAYS_AFTER_DUE,
            payer: &payer,
            message: &message,
        })
        .await;
    let code = match created {
        Ok(code) => code,
        // Every failure is treated as ambiguous: Inter may have created the charge. Recover by
        // `seuNumero` (its 30-minute duplicate guard prevents a second charge meanwhile).
        Err(error) => {
            tracing::warn!(%error, "Inter charge creation failed; looking it up by seuNumero");
            let today = billing_now().date_naive();
            match client
                .find_by_our_number(&charge.our_number, today - Duration::days(1), today)
                .await
            {
                Ok(Some(code)) => code,
                _ => return Err(AppError::Unavailable("error.billing_unavailable")),
            }
        }
    };
    let now = Utc::now().to_rfc3339();
    billing_inter::attach_request(pool, env, &charge.our_number, &code, &now).await?;
    Ok(())
}
