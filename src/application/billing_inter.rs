//! Banco Inter subscriptions: the platform owns the recurrence and issues one BolePix charge
//! per cycle. Bank state is always re-fetched from Inter before it changes coverage.

use chrono::{Duration, NaiveDate, Timelike, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::billing::{apply_events, provider_error};
use crate::domain::billing::billing_now;
use crate::domain::billing_cycle::{
    PAYABLE_DAYS_AFTER_DUE, RenewalAction, cycle_due_date, is_dead, is_paid, next_action,
    our_number,
};
use crate::domain::commercial::Quote;
use crate::domain::payments::{
    CheckoutState, GatewayEvent, PRIORITY_SETTLED, PaymentMethod, PaymentProvider,
    SubscriptionState,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;
use crate::infrastructure::database::queries::{billing as queries, billing_inter, billing_payers};
use crate::infrastructure::inter::{ChargeSnapshot, InterClient, NewCharge};

const INTER: PaymentProvider = PaymentProvider::Inter;
/// A reserved charge without a request code is retried after this long.
const RECOVERY_AFTER_MINUTES: i64 = 2;

fn environment(client: &InterClient) -> &'static str {
    client.config().environment.as_str()
}

/// First due date: today while Inter still accepts same-day charges (until 19:59), else tomorrow.
fn first_due_date() -> NaiveDate {
    let now = billing_now();
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
    let today = billing_now().date_naive();
    if !queries::reserve_contract_checkout(
        pool,
        env,
        establishment,
        &reference,
        today,
        quote,
        target,
    )
    .await?
    {
        return Ok(());
    }
    issue_cycle(
        pool,
        client,
        &reference,
        establishment,
        0,
        first_due_date(),
        quote.total_centavos,
    )
    .await
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
    let number = our_number(reference, cycle_index);
    let now = Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    billing_inter::reserve(
        &mut tx,
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
    .await?;
    tx.commit().await?;
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

async fn submit(
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

/// Applies Inter's authoritative state of one charge to coverage.
pub async fn apply_snapshot(
    pool: &SqlitePool,
    env: &str,
    snapshot: &ChargeSnapshot,
) -> AppResult<()> {
    let Some(charge) = billing_inter::find_by_request(pool, env, &snapshot.request_code).await?
    else {
        // Another system on the same Inter account; never ours.
        return Ok(());
    };
    if snapshot
        .our_number
        .as_deref()
        .is_some_and(|n| n != charge.our_number)
    {
        tracing::warn!("Inter charge returned a different seuNumero; ignored");
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    billing_inter::update_state(
        pool,
        env,
        &billing_inter::ChargeState {
            request_code: &snapshot.request_code,
            status: &snapshot.status,
            pix_copy_paste: snapshot.pix_copy_paste.as_deref(),
            digitable_line: snapshot.digitable_line.as_deref(),
            now: &now,
        },
    )
    .await?;
    let local = INTER.qualify(&charge.billing_reference);
    // Snapshots are authoritative and ordered by observation time; sub-second precision keeps a
    // later snapshot of the same charge from being discarded as "not newer".
    let at = Utc::now().format("%Y-%m-%d %H:%M:%S%.9f").to_string();
    let mut events = Vec::new();
    if is_paid(&snapshot.status) {
        if charge.cycle_index == 0 {
            events.push(GatewayEvent::Checkout {
                id: local.clone(),
                reference: Some(charge.billing_reference.clone()),
                state: CheckoutState::Paid,
            });
        }
        // Coverage requires the exact contracted amount, as for every provider.
        events.push(GatewayEvent::Payment {
            id: INTER.qualify(&snapshot.request_code),
            checkout: None,
            subscription_id: local,
            confirmed_centavos: snapshot.received_centavos,
            due_date: charge.due_date,
            priority: PRIORITY_SETTLED,
            at,
        });
    } else if is_dead(&snapshot.status) && !is_paid(&charge.status) {
        let first = charge.cycle_index == 0;
        if first {
            events.push(GatewayEvent::Checkout {
                id: local.clone(),
                reference: Some(charge.billing_reference.clone()),
                state: CheckoutState::Closed,
            });
        }
        // No provider subscription can be resumed: an unpaid cycle ends the subscription and
        // the owner subscribes again.
        events.push(GatewayEvent::Subscription {
            id: local,
            checkout: None,
            state: SubscriptionState::Deleted,
            at,
        });
    }
    if events.is_empty() {
        return Ok(());
    }
    apply_events(pool, env, None, events).await
}

/// Webhook: Inter announced changes to these charges; re-fetch each one before applying.
pub async fn handle_callback(
    pool: &SqlitePool,
    client: &InterClient,
    request_codes: &[String],
) -> AppResult<()> {
    let env = environment(client);
    for code in request_codes {
        if billing_inter::find_by_request(pool, env, code)
            .await?
            .is_none()
        {
            continue;
        }
        let snapshot = client.get_charge(code).await.map_err(provider_error)?;
        apply_snapshot(pool, env, &snapshot).await?;
    }
    Ok(())
}

/// Scheduler pass: issues upcoming cycles, lapses unpaid subscriptions and reconciles charges
/// whose callbacks may have been missed. Returns (issued, reconciled).
pub async fn run_cycle(pool: &SqlitePool, client: &InterClient) -> AppResult<(usize, usize)> {
    let env = environment(client);
    let today = billing_now().date_naive();
    let mut issued = 0;
    for candidate in billing_inter::renewal_candidates(pool, env).await? {
        match next_action(
            candidate.anchor_due_date,
            candidate.plan_months,
            candidate.last_cycle,
            &candidate.last_status,
            today,
        ) {
            RenewalAction::Wait | RenewalAction::Lapse => {}
            RenewalAction::Issue {
                cycle_index,
                due_date,
            } => {
                debug_assert_eq!(
                    due_date,
                    cycle_due_date(
                        candidate.anchor_due_date,
                        candidate.plan_months,
                        cycle_index
                    )
                );
                match issue_cycle(
                    pool,
                    client,
                    &candidate.reference,
                    &candidate.establishment_id,
                    cycle_index,
                    due_date,
                    candidate.price_centavos,
                )
                .await
                {
                    Ok(()) => issued += 1,
                    Err(error) => tracing::warn!(%error, "Inter renewal charge not issued yet"),
                }
            }
        }
    }
    let mut reconciled = 0;
    let stale_before = Utc::now() - Duration::minutes(RECOVERY_AFTER_MINUTES);
    for charge in billing_inter::open_charges(pool, env, 200).await? {
        let result = match &charge.request_code {
            None => {
                let reserved_long_ago = chrono::DateTime::parse_from_rfc3339(&charge.updated_at)
                    .is_ok_and(|at| at < stale_before);
                if reserved_long_ago {
                    submit(pool, client, &charge).await
                } else {
                    Ok(())
                }
            }
            Some(code) => match client.get_charge(code).await {
                Ok(snapshot) => apply_snapshot(pool, env, &snapshot).await,
                Err(error) => Err(provider_error(error)),
            },
        };
        match result {
            Ok(()) => reconciled += 1,
            Err(error) => tracing::warn!(%error, "Inter reconciliation skipped a charge"),
        }
    }
    Ok((issued, reconciled))
}

/// Cancels renewal: pending charges are cancelled at Inter; paid periods stay covered.
pub async fn cancel(pool: &SqlitePool, client: &InterClient, reference: &str) -> AppResult<()> {
    let env = environment(client);
    let now = Utc::now().to_rfc3339();
    for charge in billing_inter::cancellable_for_reference(pool, reference).await? {
        match &charge.request_code {
            Some(code) => {
                client
                    .cancel_charge(code, "Assinatura cancelada")
                    .await
                    .map_err(provider_error)?;
            }
            None => {
                sqlx::query(
                    "UPDATE billing_inter_charges SET status = 'CANCELADO', updated_at = ?
                     WHERE environment = ? AND our_number = ? AND request_code IS NULL",
                )
                .bind(&now)
                .bind(env)
                .bind(&charge.our_number)
                .execute(pool)
                .await?;
            }
        }
    }
    Ok(())
}
