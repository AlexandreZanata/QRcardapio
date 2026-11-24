//! Domain events written to the transactional outbox (`domain_events`).
//! `contracts/events/registry.json` documents every event; tests enforce parity.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    SubscriptionActivated,
    EntitlementGranted,
    EntitlementRevoked,
    EstablishmentSuspended,
    EstablishmentReinstated,
    CatalogPublished,
}

impl EventType {
    pub const ALL: [Self; 6] = [
        Self::SubscriptionActivated,
        Self::EntitlementGranted,
        Self::EntitlementRevoked,
        Self::EstablishmentSuspended,
        Self::EstablishmentReinstated,
        Self::CatalogPublished,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubscriptionActivated => "SubscriptionActivated",
            Self::EntitlementGranted => "EntitlementGranted",
            Self::EntitlementRevoked => "EntitlementRevoked",
            Self::EstablishmentSuspended => "EstablishmentSuspended",
            Self::EstablishmentReinstated => "EstablishmentReinstated",
            Self::CatalogPublished => "CatalogPublished",
        }
    }

    pub const fn version(self) -> u32 {
        1
    }

    pub const fn aggregate_type(self) -> &'static str {
        match self {
            Self::SubscriptionActivated => "commercial_contract",
            Self::EntitlementGranted | Self::EntitlementRevoked => "entitlement_grant",
            Self::EstablishmentSuspended
            | Self::EstablishmentReinstated
            | Self::CatalogPublished => "establishment",
        }
    }
}

/// An event ready to be appended in the same transaction as its state change.
/// Payloads must only carry identifiers and integer amounts, never personal data.
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub event_type: EventType,
    pub establishment_id: String,
    pub aggregate_id: String,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub payload: Value,
    pub dedupe_key: Option<String>,
}

impl NewEvent {
    pub fn new(
        event_type: EventType,
        establishment_id: &str,
        aggregate_id: &str,
        payload: Value,
    ) -> Self {
        Self {
            event_type,
            establishment_id: establishment_id.to_owned(),
            aggregate_id: aggregate_id.to_owned(),
            correlation_id: None,
            causation_id: None,
            payload,
            dedupe_key: None,
        }
    }

    pub fn deduplicated(mut self, key: String) -> Self {
        self.dedupe_key = Some(key);
        self
    }

    pub fn correlated(mut self, correlation_id: Option<String>) -> Self {
        self.correlation_id = correlation_id;
        self
    }
}
