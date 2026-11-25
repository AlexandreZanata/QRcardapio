//! Configured SaaS billing providers and payment-method routing. Several providers can be
//! active at once; each payment method maps to exactly one provider and each subscription stays
//! with the provider it was contracted on.

use crate::application::entitlements::Enforcement;
use crate::config::{Config, PaymentsConfig};
use crate::domain::payments::{PaymentMethod, PaymentProvider};
use crate::infrastructure::asaas::AsaasClient;
use crate::infrastructure::inter::InterClient;
use crate::infrastructure::stripe::StripeClient;

#[derive(Clone, Default)]
pub struct BillingGateways {
    pub asaas: Option<AsaasClient>,
    pub stripe: Option<StripeClient>,
    pub inter: Option<InterClient>,
    card_provider: Option<PaymentProvider>,
    environment: Option<&'static str>,
}

impl BillingGateways {
    /// Builds the clients from validated configuration. Inter needs a readable PEM identity;
    /// an invalid one disables nothing silently — it is a start-up error.
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let PaymentsConfig {
            stripe,
            inter,
            card_provider,
        } = &config.payments;
        config.payments.validate(config.asaas.as_ref())?;
        let asaas = config.asaas.clone().map(AsaasClient::new);
        let stripe = stripe.clone().map(StripeClient::new);
        let inter = inter.clone().map(InterClient::new).transpose()?;
        let environment = asaas
            .as_ref()
            .map(|c| c.config().environment.as_str())
            .or_else(|| stripe.as_ref().map(|c| c.config().environment.as_str()))
            .or_else(|| inter.as_ref().map(|c| c.config().environment.as_str()));
        let card_provider = card_provider.or(match (&asaas, &stripe) {
            (Some(_), None) => Some(PaymentProvider::Asaas),
            (None, Some(_)) => Some(PaymentProvider::Stripe),
            _ => None,
        });
        Ok(Self {
            asaas,
            stripe,
            inter,
            card_provider,
            environment,
        })
    }

    /// Replaces the Inter client (tests point it at a loopback mock).
    #[doc(hidden)]
    pub fn with_inter(mut self, inter: InterClient) -> Self {
        self.environment = self
            .environment
            .or(Some(inter.config().environment.as_str()));
        self.inter = Some(inter);
        self
    }

    /// Replaces the Stripe client (tests point it at a loopback mock).
    #[doc(hidden)]
    pub fn with_stripe(mut self, stripe: StripeClient) -> Self {
        self.environment = self
            .environment
            .or(Some(stripe.config().environment.as_str()));
        if self.card_provider.is_none() {
            self.card_provider = Some(PaymentProvider::Stripe);
        }
        self.stripe = Some(stripe);
        self
    }

    pub fn enabled(&self) -> bool {
        self.environment.is_some()
    }

    /// Shared billing environment (`sandbox` or `production`).
    pub fn environment(&self) -> Option<&'static str> {
        self.environment
    }

    pub fn enforcement(&self) -> Enforcement {
        match self.environment {
            Some(environment) => Enforcement::Enforced { environment },
            None => Enforcement::NotEnforced,
        }
    }

    pub fn provider_for(&self, method: PaymentMethod) -> Option<PaymentProvider> {
        match method {
            PaymentMethod::Card => self.card_provider,
            PaymentMethod::PixBoleto => self.inter.is_some().then_some(PaymentProvider::Inter),
        }
    }

    /// Payment methods offered to owners, in display order.
    pub fn methods(&self) -> Vec<PaymentMethod> {
        PaymentMethod::ALL
            .into_iter()
            .filter(|method| self.provider_for(*method).is_some())
            .collect()
    }
}
