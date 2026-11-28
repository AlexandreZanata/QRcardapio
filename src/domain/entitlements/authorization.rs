use super::Capability;

/// Role of a staff member inside one establishment (`establishment_members` or `establishment_member_roles`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    Owner,
    Manager,
    Cashier,
    Waiter,
}

impl MemberRole {
    pub const ALL: [Self; 4] = [Self::Owner, Self::Manager, Self::Cashier, Self::Waiter];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Manager => "manager",
            Self::Cashier => "cashier",
            Self::Waiter => "waiter",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == raw)
    }
}

/// Effective authorization role for an operation (establishment staff or storefront consumer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorRole {
    Member(MemberRole),
    Consumer,
}

impl ActorRole {
    pub const ALL: [Self; 5] = [
        Self::Member(MemberRole::Owner),
        Self::Member(MemberRole::Manager),
        Self::Member(MemberRole::Cashier),
        Self::Member(MemberRole::Waiter),
        Self::Consumer,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member(role) => role.as_str(),
            Self::Consumer => "consumer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == raw)
    }
}

impl From<MemberRole> for ActorRole {
    fn from(role: MemberRole) -> Self {
        Self::Member(role)
    }
}

/// How an operation behaves once the capability that protects it is no longer active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterExpiry {
    /// Creates new protected commercial activity.
    Deny,
    /// Free QR Core operation or reversal of existing work.
    Allow,
    /// Allowed only for work already started while the capability was active.
    CompleteExisting,
}

impl AfterExpiry {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Allow => "allow",
            Self::CompleteExisting => "complete_existing",
        }
    }
}

/// Implemented operations guarded by `application::entitlements::authorize`.
/// Planned operations live only in `contracts/entitlements/authorization-rules.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    PublishMenu,
    UnpublishMenu,
    EditMenuDraft,
    ViewBilling,
    ManageBilling,
    ManageBusinessHours,
    ManageCustomers,
    ExportData,
    EraseCustomer,
    DeliveryStorefrontPublish,
    DeliveryStorefrontUnpublish,
    DeliveryOrderPlace,
    DeliveryOrderProgress,
    DeliveryOrderRefund,
    DeliverySettingsUpdate,
    DeliveryOrderCancel,
}

const OWNER_ONLY: &[ActorRole] = &[ActorRole::Member(MemberRole::Owner)];
const OWNER_AND_MANAGER: &[ActorRole] = &[
    ActorRole::Member(MemberRole::Owner),
    ActorRole::Member(MemberRole::Manager),
];
const DELIVERY_STAFF: &[ActorRole] = &[
    ActorRole::Member(MemberRole::Owner),
    ActorRole::Member(MemberRole::Manager),
    ActorRole::Member(MemberRole::Cashier),
    ActorRole::Member(MemberRole::Waiter),
];
const CONSUMER_ONLY: &[ActorRole] = &[ActorRole::Consumer];

impl Operation {
    pub const ALL: [Self; 16] = [
        Self::PublishMenu,
        Self::UnpublishMenu,
        Self::EditMenuDraft,
        Self::ViewBilling,
        Self::ManageBilling,
        Self::ManageBusinessHours,
        Self::ManageCustomers,
        Self::ExportData,
        Self::EraseCustomer,
        Self::DeliveryStorefrontPublish,
        Self::DeliveryStorefrontUnpublish,
        Self::DeliveryOrderPlace,
        Self::DeliveryOrderProgress,
        Self::DeliveryOrderRefund,
        Self::DeliverySettingsUpdate,
        Self::DeliveryOrderCancel,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PublishMenu => "menu.publish",
            Self::UnpublishMenu => "menu.unpublish",
            Self::EditMenuDraft => "menu.draft.edit",
            Self::ViewBilling => "billing.view",
            Self::ManageBilling => "billing.manage",
            Self::ManageBusinessHours => "commerce.hours.manage",
            Self::ManageCustomers => "customers.manage",
            Self::ExportData => "data.export",
            Self::EraseCustomer => "data.customer.erase",
            Self::DeliveryStorefrontPublish => "delivery.storefront.publish",
            Self::DeliveryStorefrontUnpublish => "delivery.storefront.unpublish",
            Self::DeliveryOrderPlace => "delivery.order.place",
            Self::DeliveryOrderProgress => "delivery.order.progress",
            Self::DeliveryOrderRefund => "delivery.order.refund",
            Self::DeliverySettingsUpdate => "delivery.settings.update",
            Self::DeliveryOrderCancel => "delivery.order.cancel",
        }
    }

    pub const fn roles(self) -> &'static [ActorRole] {
        match self {
            Self::ManageBilling | Self::ExportData | Self::EraseCustomer => OWNER_ONLY,
            Self::PublishMenu
            | Self::UnpublishMenu
            | Self::EditMenuDraft
            | Self::ViewBilling
            | Self::ManageBusinessHours
            | Self::ManageCustomers
            | Self::DeliveryStorefrontPublish
            | Self::DeliveryStorefrontUnpublish
            | Self::DeliveryOrderRefund
            | Self::DeliverySettingsUpdate => OWNER_AND_MANAGER,
            Self::DeliveryOrderProgress => DELIVERY_STAFF,
            Self::DeliveryOrderPlace | Self::DeliveryOrderCancel => CONSUMER_ONLY,
        }
    }

    pub const fn capability(self) -> Option<Capability> {
        match self {
            Self::PublishMenu => Some(Capability::MenuPublish),
            Self::DeliveryStorefrontPublish => Some(Capability::DeliveryStorefrontPublish),
            Self::DeliveryOrderPlace => Some(Capability::DeliveryOrderCreate),
            Self::DeliveryOrderProgress => Some(Capability::DeliveryOrderManage),
            Self::DeliverySettingsUpdate => Some(Capability::DeliverySettingsManage),
            _ => None,
        }
    }

    pub const fn after_expiry(self) -> AfterExpiry {
        match self {
            Self::DeliveryOrderProgress => AfterExpiry::CompleteExisting,
            _ if self.capability().is_some() => AfterExpiry::Deny,
            _ => AfterExpiry::Allow,
        }
    }

    pub(crate) const fn requires_active_establishment(self) -> bool {
        matches!(self.after_expiry(), AfterExpiry::Deny)
    }

    pub fn permits(self, role: impl Into<ActorRole>) -> bool {
        let role = role.into();
        self.roles().contains(&role)
    }
}

/// Operational state of an establishment, independent of billing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalStatus {
    Active,
    SuspendedAdmin,
    SuspendedSecurity,
}

impl OperationalStatus {
    pub const ALL: [Self; 3] = [Self::Active, Self::SuspendedAdmin, Self::SuspendedSecurity];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::SuspendedAdmin => "suspended_admin",
            Self::SuspendedSecurity => "suspended_security",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == raw)
    }

    /// Suspension blocks new protected commercial activity only. Unpublishing, viewing
    /// data, completing existing work and regularizing the subscription stay available.
    pub const fn blocks(self, operation: Operation) -> bool {
        !matches!(self, Self::Active) && operation.requires_active_establishment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_are_separate_from_capabilities() {
        assert!(Operation::PublishMenu.permits(MemberRole::Manager));
        assert!(!Operation::ManageBilling.permits(MemberRole::Manager));
        assert!(Operation::ManageBilling.permits(MemberRole::Owner));
        assert!(Operation::DeliveryOrderProgress.permits(MemberRole::Cashier));
        assert!(Operation::DeliveryOrderProgress.permits(MemberRole::Waiter));
        assert!(!Operation::DeliveryOrderRefund.permits(MemberRole::Cashier));
        assert!(Operation::DeliveryOrderPlace.permits(ActorRole::Consumer));
        assert!(!Operation::DeliveryOrderPlace.permits(MemberRole::Owner));
        assert_eq!(Operation::ManageBilling.capability(), None);
        assert_eq!(
            Operation::PublishMenu.capability(),
            Some(Capability::MenuPublish)
        );
        assert_eq!(
            Operation::DeliveryOrderProgress.after_expiry(),
            AfterExpiry::CompleteExisting
        );
    }

    #[test]
    fn suspension_only_blocks_new_commercial_activity() {
        for status in [
            OperationalStatus::SuspendedAdmin,
            OperationalStatus::SuspendedSecurity,
        ] {
            assert!(status.blocks(Operation::PublishMenu));
            assert!(status.blocks(Operation::DeliveryOrderPlace));
            assert!(status.blocks(Operation::DeliveryStorefrontPublish));
            assert!(!status.blocks(Operation::UnpublishMenu));
            assert!(!status.blocks(Operation::DeliveryStorefrontUnpublish));
            assert!(!status.blocks(Operation::DeliveryOrderProgress));
            assert!(!status.blocks(Operation::DeliveryOrderRefund));
            assert!(!status.blocks(Operation::DeliveryOrderCancel));
            assert!(!status.blocks(Operation::ManageBilling));
        }
        assert!(!OperationalStatus::Active.blocks(Operation::PublishMenu));
        assert_eq!(MemberRole::parse("manager"), Some(MemberRole::Manager));
        assert_eq!(MemberRole::parse("cashier"), Some(MemberRole::Cashier));
        assert_eq!(MemberRole::parse("waiter"), Some(MemberRole::Waiter));
        assert_eq!(MemberRole::parse("consumer"), None);
        assert_eq!(ActorRole::parse("consumer"), Some(ActorRole::Consumer));
        assert_eq!(MemberRole::parse("admin"), None);
    }
}
