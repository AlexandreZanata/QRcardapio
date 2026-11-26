use std::collections::HashMap;

use sqlx::{SqliteConnection, SqliteExecutor, SqlitePool};

use super::establishments::mark_menu_draft_changed;
use super::{addon_groups, addon_options, product_variations};
use crate::domain::commerce::catalog_snapshot::CatalogAddonGroup;
use crate::domain::publication::{MenuSnapshot, ProductCatalogOptions};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn enabled(pool: &SqlitePool, est_id: &str) -> AppResult<bool> {
    enabled_in(pool, est_id).await
}

pub(super) async fn enabled_in<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public_menu_option_preferences WHERE establishment_id = ?)",
    )
    .bind(est_id)
    .fetch_one(executor)
    .await?)
}

pub async fn set_enabled(
    pool: &SqlitePool,
    est_id: &str,
    enabled: bool,
    now: &str,
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    let statement = if enabled {
        "INSERT INTO public_menu_option_preferences (establishment_id) VALUES (?)
         ON CONFLICT (establishment_id) DO NOTHING"
    } else {
        "DELETE FROM public_menu_option_preferences WHERE establishment_id = ?"
    };
    let result = sqlx::query(statement)
        .bind(est_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 0 {
        mark_menu_draft_changed(&mut tx, est_id, now).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Populate the selected products in the transaction that captured their draft data.
pub(super) async fn attach_catalog_options(
    conn: &mut SqliteConnection,
    snapshot: &mut MenuSnapshot,
) -> AppResult<()> {
    let est_id = &snapshot.establishment.id;
    let mut products: HashMap<String, ProductCatalogOptions> = HashMap::new();
    for (product_id, variation) in product_variations::list_in(conn, est_id, None).await? {
        products
            .entry(product_id)
            .or_default()
            .variations
            .push(variation);
    }
    let mut options = HashMap::new();
    for (group_id, option) in addon_options::list_in(conn, est_id, None).await? {
        options
            .entry(group_id)
            .or_insert_with(Vec::new)
            .push(option);
    }
    let groups: HashMap<_, _> = addon_groups::list_in(conn, est_id)
        .await?
        .into_iter()
        .map(|group| {
            let options = options.remove(&group.id).unwrap_or_default();
            (group.id.clone(), CatalogAddonGroup::new(&group, options))
        })
        .collect();
    for (product_id, group_id) in addon_groups::list_product_links_in(conn, est_id).await? {
        let group = groups
            .get(&group_id)
            .ok_or_else(|| AppError::Internal("catalog link references a missing group".into()))?;
        products
            .entry(product_id)
            .or_default()
            .addon_groups
            .push(group.clone());
    }

    for item in snapshot
        .categories
        .iter_mut()
        .flat_map(|section| &mut section.products)
    {
        item.catalog_options = products.remove(&item.product.id).unwrap_or_default();
    }
    Ok(())
}
