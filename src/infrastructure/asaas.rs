mod webhook;

pub use webhook::{Change, Webhook};

use std::time::Duration;

use reqwest::header::ACCEPT;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::{Number, json};
use uuid::Uuid;

use crate::config::{AsaasConfig, AsaasEnvironment};
use crate::domain::billing::{CHECKOUT_MINUTES, MONTHLY_PRICE_CENTAVOS, billing_now};

pub const BILLING_PATH: &str = "/dashboard/billing";

#[derive(Clone)]
pub struct AsaasClient {
    http: Client,
    config: AsaasConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Asaas returned HTTP {0}")]
    Status(StatusCode),
    #[error("Asaas request could not be completed")]
    Transport,
    #[error("Asaas returned an invalid response")]
    Response,
}

impl GatewayError {
    pub fn rejected_checkout(&self) -> bool {
        matches!(
            self,
            Self::Status(
                StatusCode::BAD_REQUEST
                    | StatusCode::UNAUTHORIZED
                    | StatusCode::FORBIDDEN
                    | StatusCode::UNPROCESSABLE_ENTITY
                    | StatusCode::TOO_MANY_REQUESTS
            )
        )
    }
}

impl AsaasClient {
    pub fn new(config: AsaasConfig) -> Self {
        let http = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .user_agent(concat!("QRCardapio/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Asaas HTTP client configuration is valid");
        Self { http, config }
    }

    pub fn config(&self) -> &AsaasConfig {
        &self.config
    }

    pub fn checkout_url(&self, checkout_id: Uuid) -> Url {
        let origin = self.config.environment.checkout_origin();
        let url = match self.config.environment {
            AsaasEnvironment::Sandbox => format!("{origin}/checkoutSession/show/{checkout_id}"),
            AsaasEnvironment::Production => {
                format!("{origin}/checkoutSession/show?id={checkout_id}")
            }
        };
        Url::parse(&url).expect("valid Asaas checkout URL")
    }

    pub async fn create_checkout(
        &self,
        public_base: &str,
        reference: Uuid,
    ) -> Result<Uuid, GatewayError> {
        #[derive(Deserialize)]
        struct Checkout {
            id: String,
        }

        let response = self
            .request(reqwest::Method::POST, "/checkouts")
            .json(&checkout_payload(public_base, reference))
            .send()
            .await
            .map_err(|_| GatewayError::Transport)?;
        if !response.status().is_success() {
            return Err(GatewayError::Status(response.status()));
        }
        let checkout: Checkout = response.json().await.map_err(|_| GatewayError::Response)?;
        Uuid::parse_str(&checkout.id).map_err(|_| GatewayError::Response)
    }

    pub async fn checkout_subscription(
        &self,
        checkout: Uuid,
    ) -> Result<Option<String>, GatewayError> {
        #[derive(Deserialize)]
        struct List {
            data: Vec<Charge>,
            #[serde(rename = "hasMore")]
            has_more: bool,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Charge {
            checkout_session: Option<String>,
            subscription: Option<String>,
        }

        let checkout_filter = checkout.to_string();
        let response = self
            .request(reqwest::Method::GET, "/payments")
            .query(&[
                ("checkoutSession", checkout_filter.as_str()),
                ("limit", "10"),
            ])
            .send()
            .await
            .map_err(|_| GatewayError::Transport)?;
        if !response.status().is_success() {
            return Err(GatewayError::Status(response.status()));
        }
        let list: List = response.json().await.map_err(|_| GatewayError::Response)?;
        if list.has_more {
            return Err(GatewayError::Response);
        }
        let mut subscriptions = list
            .data
            .into_iter()
            .filter(|charge| {
                charge
                    .checkout_session
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    == Some(checkout)
            })
            .filter_map(|charge| charge.subscription);
        let Some(subscription) = subscriptions.next() else {
            return Ok(None);
        };
        if !webhook::subscription_id_valid(&subscription)
            || subscriptions.any(|id| id != subscription)
        {
            return Err(GatewayError::Response);
        }
        Ok(Some(subscription))
    }

    pub async fn delete_subscription(&self, subscription_id: &str) -> Result<(), GatewayError> {
        if !webhook::subscription_id_valid(subscription_id) {
            return Err(GatewayError::Response);
        }
        let response = self
            .request(
                reqwest::Method::DELETE,
                &format!("/subscriptions/{subscription_id}"),
            )
            .send()
            .await
            .map_err(|_| GatewayError::Transport)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(GatewayError::Status(response.status()));
        }
        #[derive(Deserialize)]
        struct Deletion {
            deleted: bool,
            id: String,
        }
        let deletion: Deletion = response.json().await.map_err(|_| GatewayError::Response)?;
        if deletion.deleted && deletion.id == subscription_id {
            Ok(())
        } else {
            Err(GatewayError::Response)
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(
                method,
                format!("{}{path}", self.config.environment.api_base()),
            )
            .header("access_token", self.config.api_key())
            .header(ACCEPT, "application/json")
    }
}

fn checkout_payload(public_base: &str, reference: Uuid) -> serde_json::Value {
    let callback = format!("{public_base}{BILLING_PATH}?checkout={reference}");
    let price = format!(
        "{}.{:02}",
        MONTHLY_PRICE_CENTAVOS / 100,
        MONTHLY_PRICE_CENTAVOS % 100
    )
    .parse::<Number>()
    .expect("valid monthly price");
    json!({
        "externalReference": reference.to_string(),
        "billingTypes": ["CREDIT_CARD"],
        "chargeTypes": ["RECURRENT"],
        "minutesToExpire": CHECKOUT_MINUTES,
        "callback": {
            "successUrl": callback,
            "cancelUrl": callback,
            "expiredUrl": callback
        },
        "items": [{ "name": "QR Cardápio", "description": "Assinatura mensal", "quantity": 1, "value": price }],
        "subscription": {
            "cycle": "MONTHLY",
            "nextDueDate": billing_now().date_naive().to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_recurring_checkout_uses_the_plan_price_and_origin() {
        let reference = Uuid::nil();
        let payload = checkout_payload("https://menu.example.test", reference);
        assert_eq!(payload["billingTypes"], json!(["CREDIT_CARD"]));
        assert_eq!(payload["chargeTypes"], json!(["RECURRENT"]));
        assert_eq!(payload["items"][0]["value"].to_string(), "29.90");
        assert_eq!(payload["externalReference"], reference.to_string());
        assert_eq!(payload["subscription"]["cycle"], "MONTHLY");
        assert_eq!(
            payload["subscription"]["nextDueDate"],
            billing_now().date_naive().to_string()
        );
        assert_eq!(
            payload["callback"]["successUrl"],
            format!("https://menu.example.test/dashboard/billing?checkout={reference}")
        );
        assert!(payload.get("customerData").is_none());
    }
}
