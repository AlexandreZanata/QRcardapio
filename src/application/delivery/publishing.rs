mod files;

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::time::Instant;

use chrono::Utc;
use sqlx::SqlitePool;

use self::files::PreparedFiles;
use crate::application::entitlements::{Denial, Enforcement, authorize_with_catalog};
use crate::application::publishing::PublishLocks;
use crate::application::publishing::retired_assets::RetiredAssets;
use crate::config::Config;
use crate::domain::billing::billing_now;
use crate::domain::commerce::catalog_snapshot::CatalogContent;
use crate::domain::commercial::{CommercialCatalog, GrantPolicy};
use crate::domain::delivery::storefront::StorefrontStatus;
use crate::domain::entitlements::{Capability, MemberRole, Operation};
use crate::domain::establishment::Slug;
use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{
    business_hours, catalog_snapshots, delivery_storefronts, delivery_zones,
    find_establishment_by_id,
};
use crate::infrastructure::storage::resolve_safe_subpath;
use crate::rendering::storefront::{
    StorefrontCatalogJson, StorefrontZoneJson, build_storefront_catalog_json, storefront_categories,
};

const CHANNEL: &str = "storefront";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationKind {
    Manual,
    Derived,
}

impl PublicationKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Derived => "derived",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorefrontPublishOutcome {
    pub version: i64,
    pub catalog_version: i64,
    pub html_bytes: usize,
    pub catalog_bytes: usize,
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_storefront(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    locks: &PublishLocks,
    est_id: &str,
    role: MemberRole,
    kind: PublicationKind,
) -> AppResult<StorefrontPublishOutcome> {
    let _guard = locks
        .lock_for_publication(&config.published_tmp_dir(), est_id)
        .await?;
    let op = match kind {
        PublicationKind::Manual => Operation::DeliveryStorefrontPublish,
        PublicationKind::Derived => Operation::DeliverySettingsUpdate,
    };
    authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        role,
        op,
        billing_now().date_naive(),
    )
    .await?
    .map_err(Denial::into_error)?;

    let started = Instant::now();
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    let storefront = delivery_storefronts::get_or_default(&mut *tx, est_id).await?;
    let version = storefront
        .published_version
        .checked_add(1)
        .ok_or_else(|| AppError::Internal("storefront publication version exhausted".into()))?;
    let schedule = business_hours::load_in(&mut tx, est_id).await?;
    if !schedule.settings.delivery && !schedule.settings.pickup && kind == PublicationKind::Manual {
        return Err(AppError::Validation("delivery.mode_unavailable"));
    }
    let zones = delivery_zones::list_active_for_establishment(&mut *tx, est_id).await?;
    let previous = catalog_snapshots::current_in(&mut tx, est_id, CHANNEL).await?;
    let (mut payload, content) = match kind {
        PublicationKind::Manual => {
            let (menu, content) = catalog_snapshots::capture_in(&mut tx, est_id).await?;
            let (logo, cover, photos) = images(&menu, &content);
            let payload = build_storefront_catalog_json(
                &menu.establishment,
                &storefront,
                &content,
                &zones,
                &schedule,
                logo,
                cover,
                &photos,
            );
            (payload, content)
        }
        PublicationKind::Derived => {
            if storefront.status != StorefrontStatus::Published {
                return Err(AppError::NotFound("delivery.storefront_not_found"));
            }
            let mut payload = delivery_storefronts::published_presentation(&mut *tx, est_id)
                .await?
                .map(|json| decode_payload(&json))
                .transpose()?
                .ok_or(AppError::NotFound("delivery.storefront_not_found"))?;
            let content = previous
                .as_ref()
                .ok_or(AppError::NotFound("delivery.storefront_not_found"))?
                .decode()?;
            let photos = payload
                .categories
                .iter()
                .flat_map(|category| {
                    category.products.iter().filter_map(|product| {
                        product
                            .image_url
                            .clone()
                            .map(|url| (product.id.clone(), url))
                    })
                })
                .collect();
            payload.categories = storefront_categories(&content, &photos);
            payload.zones = zones.iter().map(StorefrontZoneJson::from).collect();
            payload.schedule = (&schedule).into();
            payload.payments = (&storefront).into();
            (payload, content)
        }
    };
    tx.commit().await?;

    let candidate =
        catalog_snapshots::reserve_version(pool, est_id, CHANNEL, &now, &content).await?;
    payload.catalog_version = candidate.version;
    payload.storefront_version = version;
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AppError::Internal(format!("storefront JSON: {error}")))?;
    let files = match PreparedFiles::prepare(config, &payload, &payload_json) {
        Ok(files) => files,
        Err(error) => {
            let elapsed_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
            let _ = delivery_storefronts::record_publication_failure(
                pool,
                est_id,
                version,
                kind.as_str(),
                &error.to_string(),
                elapsed_ms,
                &now,
            )
            .await;
            return Err(error);
        }
    };

    // Publish the catalogue version, recovery payload, and outbox event in one
    // short database transaction, after all expensive staging has succeeded.
    delivery_storefronts::record_publication_success(
        pool,
        &candidate,
        previous.map(|snapshot| snapshot.version),
        delivery_storefronts::Publication {
            storefront_version: version,
            slug: &payload.slug,
            kind: kind.as_str(),
            source_updated_at: &storefront.updated_at,
            payload_json: &payload_json,
            html_bytes: files.html_bytes as i64,
            duration_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
            asset_count: files.asset_count() as i64,
            now: &now,
        },
    )
    .await?;
    let html_bytes = files.html_bytes;
    let catalog_bytes = payload_json.len();
    // If the last rename is interrupted, republish_all renders the committed
    // public payload without reading a product or establishment draft.
    files.install()?;
    Ok(StorefrontPublishOutcome {
        version,
        catalog_version: candidate.version,
        html_bytes,
        catalog_bytes,
    })
}

fn images(
    menu: &MenuSnapshot,
    content: &CatalogContent,
) -> (Option<String>, Option<String>, HashMap<String, String>) {
    let base = format!("/s/{}/media", menu.establishment.slug);
    let logo = menu
        .logo
        .as_ref()
        .map(|asset| format!("{base}/{}", asset.filename_thumb));
    let cover = menu
        .cover_image()
        .map(|asset| format!("{base}/{}", asset.filename_large));
    let visible: std::collections::HashSet<_> = content
        .categories
        .iter()
        .flat_map(|category| category.products.iter().map(|product| product.id.as_str()))
        .collect();
    let photos = menu
        .categories
        .iter()
        .flat_map(|category| {
            category.products.iter().filter_map(|item| {
                if !visible.contains(item.product.id.as_str()) {
                    return None;
                }
                item.image.as_ref().map(|asset| {
                    (
                        item.product.id.clone(),
                        format!("{base}/{}", asset.filename_thumb),
                    )
                })
            })
        })
        .collect();
    (logo, cover, photos)
}

fn decode_payload(json: &str) -> AppResult<StorefrontCatalogJson> {
    serde_json::from_str(json)
        .map_err(|error| AppError::Internal(format!("storefront recovery payload: {error}")))
}

pub async fn republish_all(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    locks: &PublishLocks,
) -> AppResult<usize> {
    if catalog
        .capability_lifecycle(Capability::DeliveryStorefrontPublish)
        .grants()
        == GrantPolicy::Never
    {
        return Ok(0);
    }
    let mut restored = 0;
    for est_id in delivery_storefronts::published_ids(pool).await? {
        let _guard = locks
            .lock_for_publication(&config.published_tmp_dir(), &est_id)
            .await?;
        let Some(json) = delivery_storefronts::published_presentation(pool, &est_id).await? else {
            tracing::warn!(establishment_id = %est_id, "public storefront has no committed recovery payload");
            continue;
        };
        let payload = decode_payload(&json)?;
        PreparedFiles::prepare(config, &payload, &json)?.install()?;
        restored += 1;
    }
    Ok(restored)
}

pub async fn unpublish_storefront(
    pool: &SqlitePool,
    config: &Config,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    locks: &PublishLocks,
    est_id: &str,
    role: MemberRole,
) -> AppResult<()> {
    let _guard = locks
        .lock_for_publication(&config.published_tmp_dir(), est_id)
        .await?;
    authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        role,
        Operation::DeliveryStorefrontUnpublish,
        billing_now().date_naive(),
    )
    .await?
    .map_err(Denial::into_error)?;
    let establishment = find_establishment_by_id(pool, est_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let slug = Slug::validate_existing(&establishment.slug)?;
    let directory = resolve_safe_subpath(&config.storefronts_dir(), slug.as_str())?;
    let stage = config.storefronts_tmp_dir();
    fs::create_dir_all(&stage)?;
    let trash = tempfile::tempdir_in(&stage)?;
    let displaced = trash.path().join("storefront");
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let storefront = delivery_storefronts::get_or_default(&mut *tx, est_id).await?;
    if storefront.status != StorefrontStatus::Published {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    delivery_storefronts::record_unpublish_in(&mut tx, est_id, storefront.published_version, &now)
        .await?;
    let renamed = match fs::rename(&directory, &displaced) {
        Ok(()) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(AppError::Io(error)),
    };
    if let Err(error) = tx.commit().await {
        if renamed && let Err(restore_error) = fs::rename(&displaced, &directory) {
            let _ = trash.keep();
            return Err(AppError::Io(restore_error));
        }
        return Err(error.into());
    }
    RetiredAssets::new(&stage, slug.as_str()).clear();
    Ok(())
}
