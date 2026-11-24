//! Stripe adapter (card subscriptions on hosted Checkout). Plain HTTPS + form encoding, no SDK.
//! Requests and the webhook endpoint are pinned to [`API_VERSION`]; see
//! `docs/architecture/payments/STRIPE.md`.

pub mod signature;
pub mod webhook;

use std::time::Duration;

use chrono::Utc;
use reqwest::header::ACCEPT;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::Deserialize;
use uuid::Uuid;

use crate::config::StripeConfig;
use crate::domain::commercial::Quote;
use crate::infrastructure::asaas::BILLING_PATH;
use crate::infrastructure::gateway::GatewayError;

/// Stripe API version used for requests and required on the webhook endpoint.
pub const API_VERSION: &str = "2026-11-18.endive";
const API_ORIGIN: &str = "https://api.stripe.com";
/// Hosted Checkout lifetime (Stripe accepts 30 minutes to 24 hours).
const CHECKOUT_MINUTES: i64 = 35;

#[derive(Clone)]
pub struct StripeClient {
    http: Client,
    config: StripeConfig,
    origin: String,
}

/// Hosted checkout created at Stripe.
pub struct CheckoutSession {
    pub id: String,
    pub url: String,
}

impl StripeClient {
    pub fn new(config: StripeConfig) -> Self {
        Self::with_origin(config, API_ORIGIN.to_owned(), true)
    }

    /// Test-only constructor pointing to a loopback mock over HTTP.
    #[doc(hidden)]
    pub fn for_loopback_tests(config: StripeConfig, origin: &str) -> Self {
        let url = reqwest::Url::parse(origin).expect("valid loopback origin");
        assert!(
            matches!(url.host_str(), Some("127.0.0.1" | "localhost")),
            "Stripe origin override is loopback-only"
        );
        Self::with_origin(config, origin.trim_end_matches('/').to_owned(), false)
    }

    fn with_origin(config: StripeConfig, origin: String, https_only: bool) -> Self {
        let http = Client::builder()
            .https_only(https_only)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("QRCardapio/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Stripe HTTP client configuration is valid");
        Self {
            http,
            config,
            origin,
        }
    }

    pub fn config(&self) -> &StripeConfig {
        &self.config
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.origin))
            .header(reqwest::header::AUTHORIZATION, self.config.authorization())
            .header("Stripe-Version", API_VERSION)
            .header(ACCEPT, "application/json")
    }

    async fn send<T: for<'de> Deserialize<'de>>(
        request: RequestBuilder,
    ) -> Result<T, GatewayError> {
        let response = request.send().await.map_err(|_| GatewayError::Transport)?;
        if !response.status().is_success() {
            return Err(GatewayError::Status(response.status()));
        }
        response.json().await.map_err(|_| GatewayError::Response)
    }

    /// Creates a subscription-mode hosted Checkout for a backend quote. The local reference is
    /// the idempotency key, so a retried request after a timeout returns the same session.
    pub async fn create_checkout(
        &self,
        public_base: &str,
        reference: Uuid,
        establishment_id: &str,
        quote: &Quote,
    ) -> Result<CheckoutSession, GatewayError> {
        #[derive(Deserialize)]
        struct Session {
            id: String,
            url: Option<String>,
        }
        let form = checkout_form(
            public_base,
            reference,
            establishment_id,
            quote,
            Utc::now().timestamp(),
        );
        let session: Session = Self::send(
            self.request(Method::POST, "/v1/checkout/sessions")
                .header("Idempotency-Key", format!("checkout-{reference}"))
                .form(&form),
        )
        .await?;
        let url = session
            .url
            .filter(|u| u.starts_with("https://"))
            .ok_or(GatewayError::Response)?;
        if !session.id.starts_with("cs_") {
            return Err(GatewayError::Response);
        }
        Ok(CheckoutSession {
            id: session.id,
            url,
        })
    }

    /// Subscription created by a completed Checkout Session (reconciliation).
    pub async fn session_subscription(
        &self,
        session_id: &str,
    ) -> Result<Option<String>, GatewayError> {
        #[derive(Deserialize)]
        struct Session {
            subscription: Option<String>,
        }
        if !valid_id(session_id, "cs_") {
            return Err(GatewayError::Response);
        }
        let session: Session =
            Self::send(self.request(Method::GET, &format!("/v1/checkout/sessions/{session_id}")))
                .await?;
        Ok(session.subscription.filter(|id| valid_id(id, "sub_")))
    }

    /// Stops renewal while keeping the paid period (`cancel_at_period_end`).
    pub async fn cancel_at_period_end(&self, subscription_id: &str) -> Result<(), GatewayError> {
        if !valid_id(subscription_id, "sub_") {
            return Err(GatewayError::Response);
        }
        let result: Result<serde_json::Value, GatewayError> = Self::send(
            self.request(
                Method::POST,
                &format!("/v1/subscriptions/{subscription_id}"),
            )
            .header("Idempotency-Key", format!("cancel-{subscription_id}"))
            .form(&[("cancel_at_period_end", "true")]),
        )
        .await;
        match result {
            Ok(_) | Err(GatewayError::Status(StatusCode::NOT_FOUND)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Invoice paid by a payment intent (refunds and disputes no longer carry the invoice since
    /// API 2025-03-31.basil).
    pub async fn invoice_for_payment_intent(
        &self,
        payment_intent: &str,
    ) -> Result<Option<String>, GatewayError> {
        #[derive(Deserialize)]
        struct List {
            data: Vec<InvoicePayment>,
        }
        #[derive(Deserialize)]
        struct InvoicePayment {
            invoice: Option<String>,
        }
        if !valid_id(payment_intent, "pi_") {
            return Err(GatewayError::Response);
        }
        let list: List = Self::send(self.request(Method::GET, "/v1/invoice_payments").query(&[
            ("payment[type]", "payment_intent"),
            ("payment[payment_intent]", payment_intent),
            ("status", "paid"),
            ("limit", "10"),
        ]))
        .await?;
        Ok(list
            .data
            .into_iter()
            .filter_map(|p| p.invoice)
            .find(|id| valid_id(id, "in_")))
    }
}

fn checkout_form(
    public_base: &str,
    reference: Uuid,
    establishment_id: &str,
    quote: &Quote,
    now: i64,
) -> Vec<(String, String)> {
    let callback = format!("{public_base}{BILLING_PATH}?checkout={reference}");
    let reference = reference.to_string();
    let name = format!("QR Cardápio: {}", quote.offer_names());
    [
        ("mode", "subscription".to_owned()),
        (
            "line_items[0][price_data][currency]",
            quote.currency.to_ascii_lowercase(),
        ),
        (
            "line_items[0][price_data][unit_amount]",
            quote.total_centavos.to_string(),
        ),
        ("line_items[0][price_data][product_data][name]", name),
        (
            "line_items[0][price_data][recurring][interval]",
            "month".to_owned(),
        ),
        (
            "line_items[0][price_data][recurring][interval_count]",
            quote.cycle.months().to_string(),
        ),
        ("line_items[0][quantity]", "1".to_owned()),
        ("success_url", callback.clone()),
        ("cancel_url", callback),
        ("client_reference_id", reference.clone()),
        ("metadata[reference]", reference.clone()),
        ("subscription_data[metadata][reference]", reference),
        (
            "subscription_data[metadata][establishment_id]",
            establishment_id.to_owned(),
        ),
        ("expires_at", (now + CHECKOUT_MINUTES * 60).to_string()),
        ("allowed_payment_method_types[]", "card".to_owned()),
        ("locale", "pt-BR".to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

/// Stripe object ids: known prefix followed by ASCII letters, digits or underscores.
pub(crate) fn valid_id(id: &str, prefix: &str) -> bool {
    id.len() <= 255
        && id.strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::commercial::{BillingCycle, catalog};

    #[test]
    fn checkout_form_carries_backend_amount_cycle_and_reference() {
        let quote = catalog().entry_quote(BillingCycle::Semiannual);
        let form = checkout_form(
            "https://menu.example.test",
            Uuid::nil(),
            "est-1",
            &quote,
            1_000,
        );
        let get = |key: &str| form.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("mode"), Some("subscription"));
        assert_eq!(get("line_items[0][price_data][currency]"), Some("brl"));
        assert_eq!(get("line_items[0][price_data][unit_amount]"), Some("17043"));
        assert_eq!(
            get("line_items[0][price_data][recurring][interval_count]"),
            Some("6")
        );
        assert_eq!(
            get("client_reference_id"),
            Some(Uuid::nil().to_string().as_str())
        );
        assert_eq!(get("expires_at"), Some("3100"));
        assert_eq!(get("allowed_payment_method_types[]"), Some("card"));
        assert!(
            get("payment_method_types[]").is_none(),
            "removed in 2026-09-30.endive"
        );
        assert_eq!(
            get("success_url"),
            Some(
                "https://menu.example.test/dashboard/billing?checkout=00000000-0000-0000-0000-000000000000"
            )
        );
    }

    #[test]
    fn validates_stripe_identifiers() {
        assert!(valid_id("sub_1MowQVLkdIwHu7ix", "sub_"));
        assert!(!valid_id("sub_", "sub_"));
        assert!(!valid_id("sub_../x", "sub_"));
        assert!(!valid_id("cs_abc", "sub_"));
    }
}
