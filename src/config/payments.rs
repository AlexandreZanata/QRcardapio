//! Stripe and Banco Inter configuration plus payment-method routing. Asaas keeps its own
//! module (`config::billing`). All configured providers must share one environment so the
//! Entitlement Engine resolves coverage from a single `environment` value.

use std::path::PathBuf;

use reqwest::header::HeaderValue;

use super::AsaasEnvironment;
use crate::domain::payments::PaymentProvider;

#[derive(Debug, Clone)]
pub struct StripeConfig {
    pub environment: AsaasEnvironment,
    secret_key: HeaderValue,
    webhook_secret: Vec<u8>,
}

impl StripeConfig {
    /// `sk_test_`/`rk_test_` keys map to sandbox and `sk_live_`/`rk_live_` to production.
    pub fn new(secret_key: &str, webhook_secret: &str) -> Result<Self, String> {
        let environment =
            if secret_key.starts_with("sk_test_") || secret_key.starts_with("rk_test_") {
                AsaasEnvironment::Sandbox
            } else if secret_key.starts_with("sk_live_") || secret_key.starts_with("rk_live_") {
                AsaasEnvironment::Production
            } else {
                return Err("STRIPE_SECRET_KEY must be a secret or restricted Stripe key".into());
            };
        if !webhook_secret.starts_with("whsec_") || webhook_secret.len() < 20 {
            return Err(
                "STRIPE_WEBHOOK_SECRET must be the endpoint signing secret (whsec_...)".into(),
            );
        }
        let mut key = HeaderValue::from_str(&format!("Bearer {secret_key}"))
            .map_err(|_| "invalid STRIPE_SECRET_KEY")?;
        key.set_sensitive(true);
        Ok(Self {
            environment,
            secret_key: key,
            webhook_secret: webhook_secret.as_bytes().to_vec(),
        })
    }

    pub fn authorization(&self) -> HeaderValue {
        self.secret_key.clone()
    }

    pub fn webhook_secret(&self) -> &[u8] {
        &self.webhook_secret
    }

    fn from_env() -> Result<Option<Self>, String> {
        let key = std::env::var("STRIPE_SECRET_KEY").unwrap_or_default();
        let secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
        if key.is_empty() && secret.is_empty() {
            return Ok(None);
        }
        Self::new(&key, &secret).map(Some)
    }
}

#[derive(Debug, Clone)]
pub struct InterConfig {
    pub environment: AsaasEnvironment,
    pub client_id: String,
    client_secret: String,
    /// PEM certificate chain and private key of the Inter integration (mTLS).
    pub identity_pem: Vec<u8>,
    /// Current account number (`x-conta-corrente`), required when the integration has several.
    pub account: Option<String>,
    webhook_token: String,
    /// Overrides the API origin. Only loopback origins are accepted, for tests.
    pub api_origin: Option<String>,
}

impl InterConfig {
    pub fn new(
        environment: AsaasEnvironment,
        client_id: &str,
        client_secret: &str,
        identity_pem: Vec<u8>,
        account: Option<String>,
        webhook_token: &str,
    ) -> Result<Self, String> {
        if client_id.trim().is_empty() || client_secret.trim().is_empty() {
            return Err("INTER_CLIENT_ID and INTER_CLIENT_SECRET are required".into());
        }
        if webhook_token.len() < 32 || !webhook_token.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("INTER_WEBHOOK_TOKEN must have at least 32 ASCII letters or digits".into());
        }
        if account
            .as_deref()
            .is_some_and(|a| a.is_empty() || !a.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err("INTER_ACCOUNT must contain digits only".into());
        }
        Ok(Self {
            environment,
            client_id: client_id.trim().to_owned(),
            client_secret: client_secret.trim().to_owned(),
            identity_pem,
            account,
            webhook_token: webhook_token.to_owned(),
            api_origin: None,
        })
    }

    /// Test-only origin override (loopback HTTP mock without mTLS).
    #[doc(hidden)]
    pub fn with_loopback_origin(mut self, origin: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(origin).map_err(|_| "invalid origin")?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        if !loopback {
            return Err("only loopback origins may override the Inter API".into());
        }
        self.api_origin = Some(origin.trim_end_matches('/').to_owned());
        Ok(self)
    }

    pub fn client_secret(&self) -> &str {
        &self.client_secret
    }

    pub fn webhook_token(&self) -> &str {
        &self.webhook_token
    }

    fn from_env() -> Result<Option<Self>, String> {
        let env = std::env::var("INTER_ENV").unwrap_or_default();
        let client_id = std::env::var("INTER_CLIENT_ID").unwrap_or_default();
        if env.is_empty() && client_id.is_empty() {
            return Ok(None);
        }
        let environment = match env.as_str() {
            "sandbox" => AsaasEnvironment::Sandbox,
            "production" => AsaasEnvironment::Production,
            _ => return Err("INTER_ENV must be sandbox or production".into()),
        };
        let read = |name: &str| -> Result<Vec<u8>, String> {
            let path = PathBuf::from(std::env::var(name).unwrap_or_default());
            std::fs::read(&path).map_err(|_| format!("{name} must point to a readable PEM file"))
        };
        let mut identity = read("INTER_CERT_PATH")?;
        identity.push(b'\n');
        identity.extend(read("INTER_KEY_PATH")?);
        reqwest::Identity::from_pem(&identity)
            .map_err(|_| "INTER_CERT_PATH and INTER_KEY_PATH must form a valid PEM identity")?;
        let account = std::env::var("INTER_ACCOUNT")
            .ok()
            .filter(|a| !a.trim().is_empty());
        Self::new(
            environment,
            &client_id,
            &std::env::var("INTER_CLIENT_SECRET").unwrap_or_default(),
            identity,
            account,
            &std::env::var("INTER_WEBHOOK_TOKEN").unwrap_or_default(),
        )
        .map(Some)
    }
}

/// Stripe, Inter and the card routing rule. `Default` means "not configured".
#[derive(Debug, Clone, Default)]
pub struct PaymentsConfig {
    pub stripe: Option<StripeConfig>,
    pub inter: Option<InterConfig>,
    /// Provider for card payments when both Asaas and Stripe are configured.
    pub card_provider: Option<PaymentProvider>,
}

impl PaymentsConfig {
    pub(super) fn from_env(
        asaas: Option<&super::AsaasConfig>,
        cookie_secure: bool,
    ) -> Result<Self, String> {
        let stripe = StripeConfig::from_env()?;
        let inter = InterConfig::from_env()?;
        let card_provider = match std::env::var("BILLING_CARD_PROVIDER")
            .unwrap_or_default()
            .as_str()
        {
            "" => None,
            "asaas" => Some(PaymentProvider::Asaas),
            "stripe" => Some(PaymentProvider::Stripe),
            _ => return Err("BILLING_CARD_PROVIDER must be asaas or stripe".into()),
        };
        let config = Self {
            stripe,
            inter,
            card_provider,
        };
        if (config.stripe.is_some() || config.inter.is_some()) && !cookie_secure {
            return Err(
                "Stripe and Inter require an HTTPS PUBLIC_BASE_URL and COOKIE_SECURE=true".into(),
            );
        }
        config.validate(asaas)?;
        Ok(config)
    }

    /// Cross-provider rules: one shared environment and an unambiguous card route.
    pub fn validate(&self, asaas: Option<&super::AsaasConfig>) -> Result<(), String> {
        let environments: Vec<AsaasEnvironment> = asaas
            .map(|a| a.environment)
            .into_iter()
            .chain(self.stripe.as_ref().map(|s| s.environment))
            .chain(self.inter.as_ref().map(|i| i.environment))
            .collect();
        if environments.windows(2).any(|w| w[0] != w[1]) {
            return Err(
                "Asaas, Stripe and Inter must all use the same environment (sandbox or production)"
                    .into(),
            );
        }
        match (asaas.is_some(), self.stripe.is_some(), self.card_provider) {
            (true, true, None) => Err(
                "BILLING_CARD_PROVIDER is required when Asaas and Stripe are both configured"
                    .into(),
            ),
            (false, _, Some(PaymentProvider::Asaas)) => {
                Err("BILLING_CARD_PROVIDER=asaas requires Asaas".into())
            }
            (_, false, Some(PaymentProvider::Stripe)) => {
                Err("BILLING_CARD_PROVIDER=stripe requires Stripe".into())
            }
            (_, _, Some(PaymentProvider::Inter)) => Err("Inter does not process cards".into()),
            _ => Ok(()),
        }
    }
}
