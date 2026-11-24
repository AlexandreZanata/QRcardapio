use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Commercial capabilities an establishment can hold.
///
/// The registry `contracts/entitlements/capabilities.json` documents every variant and
/// `tests/commercial_contracts.rs` enforces exact parity. Access checks always name a
/// capability, never a plan or bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    MenuPublish,
    DeliveryStorefrontPublish,
    DeliveryOrderCreate,
    DeliveryOrderManage,
    DeliverySettingsManage,
    TablesSessionManage,
    TablesOrderCreate,
    TablesBillManage,
    KitchenTicketCreate,
    KitchenTicketManage,
    KitchenStationManage,
    FleetDeliveryCreate,
    FleetDeliveryAssign,
    FleetDriverManage,
    LoyaltyPointsManage,
    LoyaltyRewardManage,
    AnalyticsReportsAdvanced,
    AnalyticsImport,
    PosSaleCreate,
    PosCashRegisterManage,
    ConnectApiUse,
    ConnectWebhookManage,
    BrandingCustomDomain,
}

impl Capability {
    pub const ALL: [Self; 23] = [
        Self::MenuPublish,
        Self::DeliveryStorefrontPublish,
        Self::DeliveryOrderCreate,
        Self::DeliveryOrderManage,
        Self::DeliverySettingsManage,
        Self::TablesSessionManage,
        Self::TablesOrderCreate,
        Self::TablesBillManage,
        Self::KitchenTicketCreate,
        Self::KitchenTicketManage,
        Self::KitchenStationManage,
        Self::FleetDeliveryCreate,
        Self::FleetDeliveryAssign,
        Self::FleetDriverManage,
        Self::LoyaltyPointsManage,
        Self::LoyaltyRewardManage,
        Self::AnalyticsReportsAdvanced,
        Self::AnalyticsImport,
        Self::PosSaleCreate,
        Self::PosCashRegisterManage,
        Self::ConnectApiUse,
        Self::ConnectWebhookManage,
        Self::BrandingCustomDomain,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MenuPublish => "menu.publish",
            Self::DeliveryStorefrontPublish => "delivery.storefront.publish",
            Self::DeliveryOrderCreate => "delivery.order.create",
            Self::DeliveryOrderManage => "delivery.order.manage",
            Self::DeliverySettingsManage => "delivery.settings.manage",
            Self::TablesSessionManage => "tables.session.manage",
            Self::TablesOrderCreate => "tables.order.create",
            Self::TablesBillManage => "tables.bill.manage",
            Self::KitchenTicketCreate => "kitchen.ticket.create",
            Self::KitchenTicketManage => "kitchen.ticket.manage",
            Self::KitchenStationManage => "kitchen.station.manage",
            Self::FleetDeliveryCreate => "fleet.delivery.create",
            Self::FleetDeliveryAssign => "fleet.delivery.assign",
            Self::FleetDriverManage => "fleet.driver.manage",
            Self::LoyaltyPointsManage => "loyalty.points.manage",
            Self::LoyaltyRewardManage => "loyalty.reward.manage",
            Self::AnalyticsReportsAdvanced => "analytics.reports.advanced",
            Self::AnalyticsImport => "analytics.import",
            Self::PosSaleCreate => "pos.sale.create",
            Self::PosCashRegisterManage => "pos.cash_register.manage",
            Self::ConnectApiUse => "connect.api.use",
            Self::ConnectWebhookManage => "connect.webhook.manage",
            Self::BrandingCustomDomain => "branding.custom_domain",
        }
    }

    /// Catalog offer (module or add-on code) that owns this capability.
    pub const fn offer(self) -> &'static str {
        match self {
            Self::MenuPublish => "menu",
            Self::DeliveryStorefrontPublish
            | Self::DeliveryOrderCreate
            | Self::DeliveryOrderManage
            | Self::DeliverySettingsManage => "delivery",
            Self::TablesSessionManage | Self::TablesOrderCreate | Self::TablesBillManage => {
                "tables"
            }
            Self::KitchenTicketCreate | Self::KitchenTicketManage | Self::KitchenStationManage => {
                "kitchen"
            }
            Self::FleetDeliveryCreate | Self::FleetDeliveryAssign | Self::FleetDriverManage => {
                "fleet"
            }
            Self::LoyaltyPointsManage | Self::LoyaltyRewardManage => "loyalty",
            Self::AnalyticsReportsAdvanced | Self::AnalyticsImport => "analytics",
            Self::PosSaleCreate | Self::PosCashRegisterManage => "pos",
            Self::ConnectApiUse | Self::ConnectWebhookManage => "connect",
            Self::BrandingCustomDomain => "custom_domain",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|cap| cap.as_str() == raw)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown capability '{raw}'")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_round_trip_and_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for cap in Capability::ALL {
            assert_eq!(Capability::parse(cap.as_str()), Some(cap));
            assert!(seen.insert(cap.as_str()));
            assert!(cap.as_str().starts_with(cap.offer()) || cap.offer() == "custom_domain");
        }
        assert_eq!(Capability::parse("premium"), None);
    }
}
