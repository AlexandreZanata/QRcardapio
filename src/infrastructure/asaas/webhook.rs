use chrono::{NaiveDate, NaiveDateTime};
use reqwest::Url;
use serde::Deserialize;
use serde_json::Number;
use uuid::Uuid;

use super::BILLING_PATH;
use crate::domain::payments::{
    CheckoutState, GatewayEvent, PRIORITY_REVERSED, PRIORITY_SETTLED, SubscriptionState,
};
use crate::domain::product::parse_price_centavos;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Webhook {
    pub id: String,
    event: String,
    date_created: Option<String>,
    checkout: Option<Checkout>,
    subscription: Option<Subscription>,
    payment: Option<Payment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Checkout {
    id: String,
    external_reference: Option<String>,
    callback: Option<Callback>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Callback {
    success_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Subscription {
    id: String,
    checkout_session: Option<String>,
    status: Option<String>,
    deleted: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payment {
    id: String,
    subscription: Option<String>,
    checkout_session: Option<String>,
    due_date: Option<NaiveDate>,
    original_due_date: Option<NaiveDate>,
    value: Option<Number>,
    original_value: Option<Number>,
}

impl Webhook {
    pub fn into_change(self) -> Result<Option<GatewayEvent>, &'static str> {
        if self.id.is_empty()
            || self.id.len() > 200
            || !self.id.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("invalid event id");
        }
        match self.event.as_str() {
            "CHECKOUT_CREATED" | "CHECKOUT_PAID" | "CHECKOUT_CANCELED" | "CHECKOUT_EXPIRED" => {
                let data = self.checkout.ok_or("missing checkout")?;
                let state = match self.event.as_str() {
                    "CHECKOUT_CREATED" => CheckoutState::Open,
                    "CHECKOUT_PAID" => CheckoutState::Paid,
                    _ => CheckoutState::Closed,
                };
                let reference = data
                    .external_reference
                    .as_deref()
                    .and_then(parse_uuid)
                    .or_else(|| data.callback.as_ref().and_then(Callback::reference));
                let id = parse_uuid(&data.id).ok_or("invalid checkout id")?;
                Ok(Some(GatewayEvent::Checkout {
                    id: id.to_string(),
                    reference: reference.map(|r| r.to_string()),
                    state,
                }))
            }
            "SUBSCRIPTION_CREATED"
            | "SUBSCRIPTION_UPDATED"
            | "SUBSCRIPTION_INACTIVATED"
            | "SUBSCRIPTION_DELETED" => {
                let data = self.subscription.ok_or("missing subscription")?;
                if !subscription_id_valid(&data.id) {
                    return Err("invalid subscription id");
                }
                let state = match (self.event.as_str(), data.deleted, data.status.as_deref()) {
                    ("SUBSCRIPTION_DELETED", _, _) | (_, Some(true), _) => {
                        SubscriptionState::Deleted
                    }
                    ("SUBSCRIPTION_INACTIVATED", _, _) | (_, _, Some("INACTIVE" | "EXPIRED")) => {
                        SubscriptionState::Inactive
                    }
                    (_, _, Some("ACTIVE")) => SubscriptionState::Active,
                    _ => return Ok(None),
                };
                Ok(Some(GatewayEvent::Subscription {
                    id: data.id,
                    checkout: data
                        .checkout_session
                        .as_deref()
                        .and_then(parse_uuid)
                        .map(|id| id.to_string()),
                    state,
                    at: parse_timestamp(self.date_created.as_deref())?,
                }))
            }
            "PAYMENT_CONFIRMED"
            | "PAYMENT_RECEIVED"
            | "PAYMENT_REFUNDED"
            | "PAYMENT_CHARGEBACK_REQUESTED"
            | "PAYMENT_CHARGEBACK_DISPUTE"
            | "PAYMENT_AWAITING_CHARGEBACK_REVERSAL"
            | "PAYMENT_DELETED" => {
                let data = self.payment.ok_or("missing payment")?;
                let Some(subscription_id) = data.subscription else {
                    return Ok(None);
                };
                if !subscription_id_valid(&subscription_id) || !valid_id(&data.id, "pay_") {
                    return Err("invalid payment identifiers");
                }
                let due_date = data
                    .original_due_date
                    .or(data.due_date)
                    .ok_or("missing due date")?;
                let settled = matches!(
                    self.event.as_str(),
                    "PAYMENT_CONFIRMED" | "PAYMENT_RECEIVED"
                );
                let confirmed_centavos = if settled {
                    data.original_value
                        .as_ref()
                        .or(data.value.as_ref())
                        .and_then(|value| parse_price_centavos(&value.to_string()).ok())
                        .filter(|&centavos| centavos > 0)
                } else {
                    None
                };
                Ok(Some(GatewayEvent::Payment {
                    id: data.id,
                    checkout: data
                        .checkout_session
                        .as_deref()
                        .and_then(parse_uuid)
                        .map(|id| id.to_string()),
                    subscription_id,
                    confirmed_centavos,
                    due_date,
                    priority: if settled {
                        PRIORITY_SETTLED
                    } else {
                        PRIORITY_REVERSED
                    },
                    at: parse_timestamp(self.date_created.as_deref())?,
                }))
            }
            _ => Ok(None),
        }
    }
}

impl Callback {
    fn reference(&self) -> Option<Uuid> {
        let url = Url::parse(&self.success_url).ok()?;
        if url.path() != BILLING_PATH {
            return None;
        }
        url.query_pairs()
            .find(|(key, _)| key == "checkout")
            .and_then(|(_, value)| parse_uuid(&value))
    }
}

fn parse_uuid(raw: &str) -> Option<Uuid> {
    Uuid::parse_str(raw).ok()
}

fn parse_timestamp(raw: Option<&str>) -> Result<String, &'static str> {
    let at = raw
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
        .ok_or("invalid event timestamp")?;
    Ok(at.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub(super) fn subscription_id_valid(id: &str) -> bool {
    valid_id(id, "sub_")
}

fn valid_id(id: &str, prefix: &str) -> bool {
    id.len() <= 100
        && id.strip_prefix(prefix).is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}
