use sqlx::SqlitePool;

use crate::application::entitlements::{Enforcement, authorize_with_catalog};
use crate::domain::billing::billing_now;
use crate::domain::commercial::{CommercialCatalog, GrantPolicy};
use crate::domain::entitlements::{Capability, MemberRole, Operation};
use crate::error::AppResult;
use crate::infrastructure::database::queries::delivery_storefronts;

#[derive(Default)]
pub struct StaffAccess {
    pub orders: bool,
    pub new_orders: bool,
    pub settings: bool,
    pub storefront_management: bool,
}

pub async fn staff_access(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    establishment_id: &str,
    role: MemberRole,
) -> AppResult<StaffAccess> {
    if catalog
        .capability_lifecycle(Capability::DeliveryOrderManage)
        .grants()
        == GrantPolicy::Never
    {
        return Ok(StaffAccess::default());
    }

    let today = billing_now().date_naive();
    let settings = authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        establishment_id,
        role,
        Operation::DeliverySettingsUpdate,
        today,
    )
    .await?
    .is_ok();
    let progress = authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        establishment_id,
        role,
        Operation::DeliveryOrderProgress,
        today,
    )
    .await?
    .is_ok();
    let has_history = delivery_storefronts::get_or_default(pool, establishment_id)
        .await?
        .published_version
        > 0;
    Ok(StaffAccess {
        orders: progress || settings || has_history,
        storefront_management: settings
            || (has_history && Operation::DeliveryStorefrontUnpublish.permits(role)),
        new_orders: progress,
        settings,
    })
}
