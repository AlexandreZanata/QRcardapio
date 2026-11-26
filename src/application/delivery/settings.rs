use chrono::Utc;
use sqlx::SqlitePool;

use super::publishing::{PublicationKind, publish_storefront};
use crate::application::entitlements::{Denial, Enforcement, authorize_with_catalog};
use crate::application::publishing::PublishLocks;
use crate::config::Config;
use crate::domain::billing::billing_now;
use crate::domain::commerce::business_hours::CommercialSettings;
use crate::domain::commerce::customer::normalize_phone;
use crate::domain::commercial::CommercialCatalog;
use crate::domain::delivery::coupon::{Coupon, CouponInput, ValidatedCoupon};
use crate::domain::delivery::storefront::{
    StorefrontSettings, StorefrontSettingsInput, StorefrontStatus, ValidatedStorefrontSettings,
};
use crate::domain::delivery::zone::{DeliveryZone, DeliveryZoneInput, ValidatedDeliveryZone};
use crate::domain::entitlements::{MemberRole, Operation};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    business_hours, delivery_coupons, delivery_orders, delivery_storefronts, delivery_zones,
};

pub struct DeliverySettingsView {
    pub storefront: StorefrontSettings,
    pub commercial: CommercialSettings,
    pub zones: Vec<DeliveryZone>,
    pub coupons: Vec<Coupon>,
}

pub async fn load_settings_view(
    pool: &SqlitePool,
    est_id: &str,
) -> AppResult<DeliverySettingsView> {
    let storefront = delivery_storefronts::get_or_default(pool, est_id).await?;
    let schedule = business_hours::load(pool, est_id).await?;
    let zones = delivery_zones::list_for_establishment(pool, est_id).await?;
    let coupons = delivery_coupons::list_for_establishment(pool, est_id).await?;
    Ok(DeliverySettingsView {
        storefront,
        commercial: schedule.settings,
        zones,
        coupons,
    })
}

async fn authorize_settings(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
) -> AppResult<()> {
    authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        role,
        Operation::DeliverySettingsUpdate,
        billing_now().date_naive(),
    )
    .await?
    .map_err(Denial::into_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn save_settings(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    locks: &PublishLocks,
    est_id: &str,
    role: MemberRole,
    input: StorefrontSettingsInput<'_>,
) -> AppResult<StorefrontSettings> {
    authorize_settings(pool, catalog, enforcement, est_id, role).await?;
    let validated = ValidatedStorefrontSettings::validate(input)?;
    let now = Utc::now().to_rfc3339();
    let updated = delivery_storefronts::upsert_settings(pool, est_id, &validated, &now).await?;
    if updated.status == StorefrontStatus::Published {
        publish_storefront(
            pool,
            config,
            catalog,
            enforcement,
            locks,
            est_id,
            role,
            PublicationKind::Derived,
        )
        .await?;
    }
    Ok(updated)
}

#[allow(clippy::too_many_arguments)]
pub async fn save_zone(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    locks: &PublishLocks,
    est_id: &str,
    role: MemberRole,
    zone_id: Option<&str>,
    input: DeliveryZoneInput<'_>,
) -> AppResult<DeliveryZone> {
    authorize_settings(pool, catalog, enforcement, est_id, role).await?;
    let validated = ValidatedDeliveryZone::validate(input)?;
    let now = Utc::now().to_rfc3339();
    let zone = match zone_id.filter(|id| !id.trim().is_empty()) {
        Some(id) => delivery_zones::update(pool, est_id, id, &validated, &now).await?,
        None => delivery_zones::create(pool, est_id, &validated, &now).await?,
    };
    republish_if_live(pool, config, catalog, enforcement, locks, est_id, role).await?;
    Ok(zone)
}

#[allow(clippy::too_many_arguments)]
pub async fn delete_zone(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    locks: &PublishLocks,
    est_id: &str,
    role: MemberRole,
    zone_id: &str,
) -> AppResult<()> {
    authorize_settings(pool, catalog, enforcement, est_id, role).await?;
    if !delivery_zones::delete(pool, est_id, zone_id).await? {
        return Err(AppError::NotFound("error.not_found"));
    }
    republish_if_live(pool, config, catalog, enforcement, locks, est_id, role).await?;
    Ok(())
}

async fn republish_if_live(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    locks: &PublishLocks,
    est_id: &str,
    role: MemberRole,
) -> AppResult<()> {
    let current = delivery_storefronts::get_or_default(pool, est_id).await?;
    delivery_storefronts::mark_unpublished_changes(pool, est_id, &Utc::now().to_rfc3339()).await?;
    if current.status == StorefrontStatus::Published {
        publish_storefront(
            pool,
            config,
            catalog,
            enforcement,
            locks,
            est_id,
            role,
            PublicationKind::Derived,
        )
        .await?;
    }
    Ok(())
}

pub async fn save_coupon(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
    coupon_id: Option<&str>,
    input: CouponInput<'_>,
) -> AppResult<Coupon> {
    authorize_settings(pool, catalog, enforcement, est_id, role).await?;
    let validated = ValidatedCoupon::validate(input)?;
    let now = Utc::now().to_rfc3339();
    match coupon_id.filter(|id| !id.trim().is_empty()) {
        Some(id) => delivery_coupons::update(pool, est_id, id, &validated, &now).await,
        None => delivery_coupons::create(pool, est_id, &validated, &now).await,
    }
}

pub async fn delete_coupon(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
    coupon_id: &str,
) -> AppResult<()> {
    authorize_settings(pool, catalog, enforcement, est_id, role).await?;
    if !delivery_coupons::delete(pool, est_id, coupon_id).await? {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn set_customer_block(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
    actor_account_id: &str,
    phone: &str,
    block: bool,
    reason: &str,
) -> AppResult<()> {
    authorize_settings(pool, catalog, enforcement, est_id, role).await?;
    let normalized = normalize_phone(phone)?;
    let clean_reason = reason.trim();
    if clean_reason.chars().count() > 200 {
        return Err(AppError::Validation("delivery.invalid_reason"));
    }
    let now = Utc::now().to_rfc3339();
    delivery_orders::set_phone_blocked(
        pool,
        est_id,
        &normalized,
        block,
        clean_reason,
        actor_account_id,
        &now,
    )
    .await
}
