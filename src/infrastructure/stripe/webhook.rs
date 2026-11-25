//! Stripe snapshot events (API `2026-11-18.endive`) translated into provider-neutral facts.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::Deserialize;
use serde_json::Value;

use super::valid_id;
use crate::domain::payments::{
    CheckoutState, GatewayEvent, PRIORITY_SETTLED, PaymentProvider, SubscriptionState,
};

const STRIPE: PaymentProvider = PaymentProvider::Stripe;

#[derive(Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    kind: String,
    created: i64,
    pub livemode: bool,
    data: EventData,
}

#[derive(Deserialize)]
struct EventData {
    object: Value,
}

/// What the application must do with one Stripe event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Apply(Vec<GatewayEvent>),
    /// A refund or dispute on a payment intent; the invoice is resolved through the API.
    Charge {
        payment_intent: String,
        restore: bool,
        at: String,
    },
    Ignore,
}

fn text<'a>(object: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(object, |value, key| value.get(*key))
        .and_then(Value::as_str)
}

fn timestamp(unix: i64) -> Option<String> {
    DateTime::from_timestamp(unix, 0).map(|at| at.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Billing dates follow UTC-03:00 like every other provider.
fn billing_date(unix: i64) -> Option<NaiveDate> {
    let offset = FixedOffset::west_opt(3 * 3600)?;
    DateTime::from_timestamp(unix, 0).map(|at| at.with_timezone(&offset).date_naive())
}

impl Event {
    pub fn dedupe_key(&self) -> Option<String> {
        valid_id(&self.id, "evt_").then(|| STRIPE.qualify(&self.id))
    }

    pub fn into_parsed(self) -> Result<Parsed, &'static str> {
        let at = timestamp(self.created).ok_or("invalid event timestamp")?;
        let object = &self.data.object;
        match self.kind.as_str() {
            "checkout.session.completed" | "checkout.session.async_payment_succeeded" => {
                if text(object, &["mode"]) != Some("subscription") {
                    return Ok(Parsed::Ignore);
                }
                let session = text(object, &["id"]).filter(|id| valid_id(id, "cs_"));
                let session = session.ok_or("invalid checkout session")?;
                let reference = text(object, &["client_reference_id"])
                    .and_then(|r| uuid::Uuid::parse_str(r).ok())
                    .map(|r| r.to_string());
                let mut events = vec![GatewayEvent::Checkout {
                    id: STRIPE.qualify(session),
                    reference,
                    state: CheckoutState::Paid,
                }];
                if let Some(subscription) =
                    text(object, &["subscription"]).filter(|id| valid_id(id, "sub_"))
                {
                    events.push(GatewayEvent::Subscription {
                        id: STRIPE.qualify(subscription),
                        checkout: Some(STRIPE.qualify(session)),
                        state: SubscriptionState::Active,
                        at,
                    });
                }
                Ok(Parsed::Apply(events))
            }
            "checkout.session.expired" => {
                let session = text(object, &["id"]).filter(|id| valid_id(id, "cs_"));
                let session = session.ok_or("invalid checkout session")?;
                Ok(Parsed::Apply(vec![GatewayEvent::Checkout {
                    id: STRIPE.qualify(session),
                    reference: None,
                    state: CheckoutState::Closed,
                }]))
            }
            "customer.subscription.created"
            | "customer.subscription.updated"
            | "customer.subscription.deleted" => {
                let id = text(object, &["id"]).filter(|id| valid_id(id, "sub_"));
                let id = id.ok_or("invalid subscription")?;
                let cancelling =
                    object.get("cancel_at_period_end").and_then(Value::as_bool) == Some(true);
                let state = match (self.kind.as_str(), text(object, &["status"])) {
                    ("customer.subscription.deleted", _)
                    | (_, Some("canceled" | "incomplete_expired")) => SubscriptionState::Deleted,
                    _ if cancelling => SubscriptionState::Deleted,
                    (_, Some("active" | "trialing")) => SubscriptionState::Active,
                    (_, Some("past_due" | "unpaid" | "paused")) => SubscriptionState::Inactive,
                    _ => return Ok(Parsed::Ignore),
                };
                Ok(Parsed::Apply(vec![GatewayEvent::Subscription {
                    id: STRIPE.qualify(id),
                    checkout: None,
                    state,
                    at,
                }]))
            }
            "invoice.paid" => parse_invoice(object, at),
            "charge.refunded" => {
                let fully = object.get("refunded").and_then(Value::as_bool) == Some(true);
                charge(object, !fully, false, at)
            }
            "charge.dispute.created" => charge(object, false, false, at),
            "charge.dispute.closed" => {
                let restore = matches!(text(object, &["status"]), Some("won" | "warning_closed"));
                charge(object, false, restore, at)
            }
            _ => Ok(Parsed::Ignore),
        }
    }
}

fn parse_invoice(object: &Value, at: String) -> Result<Parsed, &'static str> {
    if text(object, &["parent", "type"]) != Some("subscription_details") {
        return Ok(Parsed::Ignore);
    }
    let invoice = text(object, &["id"]).filter(|id| valid_id(id, "in_"));
    let invoice = invoice.ok_or("invalid invoice")?;
    let subscription = text(object, &["parent", "subscription_details", "subscription"])
        .filter(|id| valid_id(id, "sub_"))
        .ok_or("invoice without subscription")?;
    let period_start = object
        .get("lines")
        .and_then(|lines| lines.get("data"))
        .and_then(Value::as_array)
        .and_then(|lines| {
            lines
                .iter()
                .find(|line| text(line, &["parent", "type"]) == Some("subscription_item_details"))
        })
        .and_then(|line| line.get("period")?.get("start")?.as_i64())
        .ok_or("invoice without subscription period")?;
    let due_date = billing_date(period_start).ok_or("invalid period")?;
    let settled = text(object, &["status"]) == Some("paid")
        && text(object, &["currency"]) == Some("brl")
        && object.get("amount_remaining").and_then(Value::as_i64) == Some(0);
    let confirmed = object
        .get("amount_paid")
        .and_then(Value::as_i64)
        .filter(|amount| settled && *amount > 0);
    let Some(confirmed) = confirmed else {
        return Ok(Parsed::Ignore);
    };
    Ok(Parsed::Apply(vec![GatewayEvent::Payment {
        id: STRIPE.qualify(invoice),
        checkout: None,
        subscription_id: STRIPE.qualify(subscription),
        confirmed_centavos: Some(confirmed),
        due_date,
        priority: PRIORITY_SETTLED,
        at,
    }]))
}

fn charge(
    object: &Value,
    partial: bool,
    restore: bool,
    at: String,
) -> Result<Parsed, &'static str> {
    if partial {
        // Partial refunds keep coverage (same policy as Asaas partial refunds).
        return Ok(Parsed::Ignore);
    }
    let Some(payment_intent) = text(object, &["payment_intent"]).filter(|id| valid_id(id, "pi_"))
    else {
        return Ok(Parsed::Ignore);
    };
    Ok(Parsed::Charge {
        payment_intent: payment_intent.to_owned(),
        restore,
        at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, object: Value) -> Event {
        serde_json::from_value(json!({
            "id": "evt_1", "type": kind, "created": 1_798_765_432, "livemode": false,
            "data": {"object": object}
        }))
        .unwrap()
    }

    #[test]
    fn completed_checkout_links_session_and_subscription() {
        let parsed = event(
            "checkout.session.completed",
            json!({"id": "cs_test_a", "mode": "subscription", "subscription": "sub_9",
                   "client_reference_id": "0193a0c0-0000-7000-8000-00000000000a"}),
        )
        .into_parsed()
        .unwrap();
        let Parsed::Apply(events) = parsed else {
            panic!("expected events")
        };
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[1], GatewayEvent::Subscription { id, checkout: Some(c), .. }
            if id == "stripe:sub_9" && c == "stripe:cs_test_a")
        );
    }

    #[test]
    fn paid_invoice_uses_the_line_period_in_billing_time() {
        let invoice = json!({
            "id": "in_1", "status": "paid", "currency": "brl", "amount_paid": 2990, "amount_remaining": 0,
            "parent": {"type": "subscription_details", "subscription_details": {"subscription": "sub_9"}},
            "lines": {"data": [{"parent": {"type": "subscription_item_details"}, "period": {"start": 1_798_772_400, "end": 1_801_450_800}}]}
        });
        let Parsed::Apply(events) = event("invoice.paid", invoice).into_parsed().unwrap() else {
            panic!("expected payment")
        };
        assert!(
            matches!(&events[0], GatewayEvent::Payment { id, subscription_id, confirmed_centavos: Some(2990), due_date, .. }
            if id == "stripe:in_1" && subscription_id == "stripe:sub_9"
               && *due_date == NaiveDate::from_ymd_opt(2027, 1, 1).unwrap())
        );
        let unpaid = json!({"id": "in_2", "status": "open", "currency": "brl", "amount_paid": 0, "amount_remaining": 2990,
            "parent": {"type": "subscription_details", "subscription_details": {"subscription": "sub_9"}},
            "lines": {"data": [{"parent": {"type": "subscription_item_details"}, "period": {"start": 1, "end": 2}}]}});
        assert_eq!(
            event("invoice.paid", unpaid).into_parsed().unwrap(),
            Parsed::Ignore
        );
    }

    #[test]
    fn subscription_status_and_cancellation_map_to_local_states() {
        let state = |kind: &str, object: Value| match event(kind, object).into_parsed().unwrap() {
            Parsed::Apply(events) => match &events[0] {
                GatewayEvent::Subscription { state, .. } => *state,
                _ => panic!("subscription expected"),
            },
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(
            state(
                "customer.subscription.updated",
                json!({"id": "sub_1", "status": "active"})
            ),
            SubscriptionState::Active
        );
        assert_eq!(
            state(
                "customer.subscription.updated",
                json!({"id": "sub_1", "status": "past_due"})
            ),
            SubscriptionState::Inactive
        );
        assert_eq!(
            state(
                "customer.subscription.updated",
                json!({"id": "sub_1", "status": "active", "cancel_at_period_end": true})
            ),
            SubscriptionState::Deleted
        );
        assert_eq!(
            state(
                "customer.subscription.deleted",
                json!({"id": "sub_1", "status": "canceled"})
            ),
            SubscriptionState::Deleted
        );
    }

    #[test]
    fn refunds_and_disputes_need_the_invoice_lookup() {
        let full = event(
            "charge.refunded",
            json!({"refunded": true, "payment_intent": "pi_1"}),
        );
        assert!(matches!(
            full.into_parsed().unwrap(),
            Parsed::Charge { restore: false, .. }
        ));
        let partial = event(
            "charge.refunded",
            json!({"refunded": false, "payment_intent": "pi_1"}),
        );
        assert_eq!(partial.into_parsed().unwrap(), Parsed::Ignore);
        let won = event(
            "charge.dispute.closed",
            json!({"status": "won", "payment_intent": "pi_1"}),
        );
        assert!(matches!(
            won.into_parsed().unwrap(),
            Parsed::Charge { restore: true, .. }
        ));
        let lost = event(
            "charge.dispute.closed",
            json!({"status": "lost", "payment_intent": "pi_1"}),
        );
        assert!(matches!(
            lost.into_parsed().unwrap(),
            Parsed::Charge { restore: false, .. }
        ));
    }
}
