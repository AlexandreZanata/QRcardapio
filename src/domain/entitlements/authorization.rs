use super::Capability;

/// Role of a member inside one establishment (`establishment_members.member_role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    Owner,
    Manager,
}

impl MemberRole {
    pub const ALL: [Self; 2] = [Self::Owner, Self::Manager];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Manager => "manager",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == raw)
    }
}

/// How an operation behaves once the capability that protects it is no longer active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterExpiry {
    /// Creates new protected commercial activity.
    Deny,
    /// Free QR Core operation or reversal of existing work.
    Allow,
    /// For future channel operations that verify the work began before expiry.
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
}

impl Operation {
    pub const ALL: [Self; 9] = [
        Self::PublishMenu,
        Self::UnpublishMenu,
        Self::EditMenuDraft,
        Self::ViewBilling,
        Self::ManageBilling,
        Self::ManageBusinessHours,
        Self::ManageCustomers,
        Self::ExportData,
        Self::EraseCustomer,
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
        }
    }

    pub const fn roles(self) -> &'static [MemberRole] {
        match self {
            Self::ManageBilling | Self::ExportData | Self::EraseCustomer => &[MemberRole::Owner],
            Self::PublishMenu
            | Self::UnpublishMenu
            | Self::EditMenuDraft
            | Self::ViewBilling
            | Self::ManageBusinessHours
            | Self::ManageCustomers => &[MemberRole::Owner, MemberRole::Manager],
        }
    }

    pub const fn capability(self) -> Option<Capability> {
        match self {
            Self::PublishMenu => Some(Capability::MenuPublish),
            _ => None,
        }
    }

    pub const fn after_expiry(self) -> AfterExpiry {
        match self.capability() {
            Some(_) => AfterExpiry::Deny,
            None => AfterExpiry::Allow,
        }
    }

    pub fn permits(self, role: MemberRole) -> bool {
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
    /// data and regularizing the subscription stay available.
    pub const fn blocks(self, operation: Operation) -> bool {
        !matches!(self, Self::Active) && matches!(operation.after_expiry(), AfterExpiry::Deny)
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
        assert_eq!(Operation::ManageBilling.capability(), None);
        assert_eq!(
            Operation::PublishMenu.capability(),
            Some(Capability::MenuPublish)
        );
    }

    #[test]
    fn suspension_only_blocks_new_commercial_activity() {
        for status in [
            OperationalStatus::SuspendedAdmin,
            OperationalStatus::SuspendedSecurity,
        ] {
            assert!(status.blocks(Operation::PublishMenu));
            assert!(!status.blocks(Operation::UnpublishMenu));
            assert!(!status.blocks(Operation::ManageBilling));
        }
        assert!(!OperationalStatus::Active.blocks(Operation::PublishMenu));
        assert_eq!(MemberRole::parse("manager"), Some(MemberRole::Manager));
        assert_eq!(MemberRole::parse("admin"), None);
    }
}
