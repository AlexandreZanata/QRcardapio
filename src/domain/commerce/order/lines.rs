use std::collections::HashMap;

use super::OrderItemSelection;
use crate::domain::commerce::catalog_options::{AddonOption, ProductVariation};
use crate::domain::commerce::catalog_snapshot::{
    CatalogAddonGroup, CatalogContent, CatalogProduct,
};
use crate::domain::commerce::pricing::{AddonPrice, LinePrice, price_line};

#[derive(Debug)]
pub(crate) struct OrderItemSnapshot<'a> {
    pub product: &'a CatalogProduct,
    pub variation: Option<&'a ProductVariation>,
    pub base_price_centavos: i64,
    pub quantity: i64,
    pub price: LinePrice,
    pub addons: Vec<OrderAddonSnapshot<'a>>,
}

#[derive(Debug)]
pub(crate) struct OrderAddonSnapshot<'a> {
    pub group: &'a CatalogAddonGroup,
    pub option: &'a AddonOption,
    pub quantity: i64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LineError {
    InvalidSelection,
    Price,
}

pub(crate) fn snapshot_lines<'a>(
    catalog: &'a CatalogContent,
    selections: &[OrderItemSelection],
) -> Result<Vec<OrderItemSnapshot<'a>>, LineError> {
    let products: HashMap<_, _> = catalog
        .categories
        .iter()
        .flat_map(|category| &category.products)
        .filter(|product| product.is_available)
        .map(|product| (product.id.as_str(), product))
        .collect();
    selections
        .iter()
        .map(|item| {
            let product = products
                .get(item.product_id.as_str())
                .ok_or(LineError::InvalidSelection)?;
            let variation = selected_variation(product, item.variation_id.as_deref())?;
            let addons = selected_addons(product, item)?;
            let base_price_centavos =
                variation.map_or(product.price_centavos, |option| option.price_centavos);
            let addon_prices: Vec<_> = addons
                .iter()
                .map(|addon| AddonPrice {
                    price_centavos: addon.option.price_centavos,
                    quantity: addon.quantity,
                })
                .collect();
            let price = price_line(base_price_centavos, item.quantity, &addon_prices)
                .map_err(|_| LineError::Price)?;
            Ok(OrderItemSnapshot {
                product,
                variation,
                base_price_centavos,
                quantity: item.quantity,
                price,
                addons,
            })
        })
        .collect()
}

fn selected_variation<'a>(
    product: &'a CatalogProduct,
    id: Option<&str>,
) -> Result<Option<&'a ProductVariation>, LineError> {
    let mut available = product
        .variations
        .iter()
        .filter(|option| option.is_available);
    match id {
        Some(id) => available
            .find(|option| option.id == id)
            .map(Some)
            .ok_or(LineError::InvalidSelection),
        None if available.next().is_none() => Ok(None),
        None => Err(LineError::InvalidSelection),
    }
}

fn selected_addons<'a>(
    product: &'a CatalogProduct,
    item: &OrderItemSelection,
) -> Result<Vec<OrderAddonSnapshot<'a>>, LineError> {
    let mut remaining = HashMap::new();
    for addon in &item.addons {
        if remaining
            .insert(addon.option_id.as_str(), addon.quantity)
            .is_some()
        {
            return Err(LineError::InvalidSelection);
        }
    }

    let mut addons = Vec::new();
    for group in &product.addon_groups {
        let mut selected_count = 0;
        for option in &group.options {
            let Some(quantity) = remaining.remove(option.id.as_str()) else {
                continue;
            };
            if !option.is_available || !(1..=option.max_quantity).contains(&quantity) {
                return Err(LineError::InvalidSelection);
            }
            addons.push(OrderAddonSnapshot {
                group,
                option,
                quantity,
            });
            selected_count += 1;
        }
        if !(group.min_select..=group.max_select).contains(&selected_count) {
            return Err(LineError::InvalidSelection);
        }
    }
    if !remaining.is_empty() {
        return Err(LineError::InvalidSelection);
    }
    Ok(addons)
}
