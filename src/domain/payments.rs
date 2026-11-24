//! Provider-neutral SaaS billing vocabulary shared by the Asaas, Stripe and Inter adapters.
//!
//! Every gateway webhook or reconciliation is translated into a [`GatewayEvent`] and applied by
//! `application::billing`, so coverage, entitlements and affiliate commissions never depend on
//! which provider collected the money.

use chrono::NaiveDate;

/// Payment provider that collects one SaaS subscription. A subscription never moves between
/// providers; changing provider means a new contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaymentProvider {
    Asaas,
    Stripe,
    Inter,
}

impl PaymentProvider {
    pub const ALL: [Self; 3] = [Self::Asaas, Self::Stripe, Self::Inter];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asaas => "asaas",
            Self::Stripe => "stripe",
            Self::Inter => "inter",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == raw)
    }

    /// Identifiers of new providers are stored as `provider:raw` so the legacy unique keys on
    /// `billing_*` tables stay globally unique (Asaas and Stripe both use `sub_`/`evt_` ids).
    /// Asaas identifiers keep their historical, unqualified form.
    pub fn qualify(self, raw: &str) -> String {
        match self {
            Self::Asaas => raw.to_owned(),
            Self::Stripe | Self::Inter => format!("{}:{raw}", self.as_str()),
        }
    }

    /// Inverse of [`qualify`](Self::qualify); `None` when the stored id belongs to another provider.
    pub fn unqualify(self, stored: &str) -> Option<&str> {
        match self {
            Self::Asaas => (!stored.contains(':')).then_some(stored),
            Self::Stripe | Self::Inter => stored
                .strip_prefix(self.as_str())
                .and_then(|rest| rest.strip_prefix(':')),
        }
    }
}

/// Payment method offered to the customer. Each method is routed to exactly one provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaymentMethod {
    /// Recurring credit card on a hosted checkout (Stripe or Asaas).
    Card,
    /// One Inter charge per cycle payable by Pix or boleto (BolePix).
    PixBoleto,
}

impl PaymentMethod {
    pub const ALL: [Self; 2] = [Self::Card, Self::PixBoleto];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::PixBoleto => "pix_boleto",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == raw)
    }

    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Card => "billing.method_card",
            Self::PixBoleto => "billing.method_pix_boleto",
        }
    }
}

/// Local checkout state (`billing_checkouts.checkout_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutState {
    Open,
    Paid,
    Closed,
}

impl CheckoutState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Paid => "paid",
            Self::Closed => "closed",
        }
    }
}

/// Local subscription state (`billing_checkouts.subscription_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionState {
    Active,
    Inactive,
    Deleted,
}

impl SubscriptionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Deleted => "deleted",
        }
    }
}

/// Settlements win over reversals only when they are strictly newer (same rule as Asaas).
pub const PRIORITY_SETTLED: i64 = 1;
pub const PRIORITY_REVERSED: i64 = 2;

/// A normalized gateway fact. Identifiers are already provider-qualified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayEvent {
    Checkout {
        id: String,
        reference: Option<String>,
        state: CheckoutState,
    },
    Subscription {
        id: String,
        checkout: Option<String>,
        state: SubscriptionState,
        at: String,
    },
    Payment {
        id: String,
        checkout: Option<String>,
        subscription_id: String,
        /// `Some` only for a settled payment; reversals and pending charges carry `None`.
        confirmed_centavos: Option<i64>,
        due_date: NaiveDate,
        priority: i64,
        at: String,
    },
    /// Full refund, chargeback or lost dispute of a known payment (Stripe resolves the charge
    /// to its invoice before emitting this).
    PaymentReversed { payment_id: String, at: String },
    /// Dispute won: the amount originally settled is restored.
    PaymentRestored { payment_id: String, at: String },
}

/// Formats integer centavos as a gateway decimal string (`2990` -> `29.90`).
pub fn centavos_to_decimal(centavos: i64) -> String {
    debug_assert!(centavos >= 0);
    format!("{}.{:02}", centavos / 100, centavos % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_qualified_per_provider() {
        assert_eq!(PaymentProvider::Asaas.qualify("sub_1"), "sub_1");
        assert_eq!(PaymentProvider::Stripe.qualify("sub_1"), "stripe:sub_1");
        assert_eq!(
            PaymentProvider::Stripe.unqualify("stripe:sub_1"),
            Some("sub_1")
        );
        assert_eq!(PaymentProvider::Stripe.unqualify("sub_1"), None);
        assert_eq!(PaymentProvider::Asaas.unqualify("stripe:sub_1"), None);
        assert_eq!(PaymentProvider::Inter.unqualify("inter:abc"), Some("abc"));
        assert_eq!(PaymentProvider::Inter.unqualify("stripe:abc"), None);
        for provider in PaymentProvider::ALL {
            assert_eq!(PaymentProvider::parse(provider.as_str()), Some(provider));
        }
    }

    #[test]
    fn formats_amounts_without_floats() {
        assert_eq!(centavos_to_decimal(2990), "29.90");
        assert_eq!(centavos_to_decimal(32_292), "322.92");
        assert_eq!(centavos_to_decimal(5), "0.05");
    }
}
