use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::publication::MenuSnapshot;
use crate::error::{AppError, AppResult};

use super::catalog_options::{AddonGroup, AddonOption, ProductVariation};

const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityTarget<'a> {
    Product(&'a str),
    Variation {
        product_id: &'a str,
        variation_id: &'a str,
    },
    AddonOption {
        group_id: &'a str,
        option_id: &'a str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogContent {
    schema_version: u8,
    pub categories: Vec<CatalogCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogCategory {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub products: Vec<CatalogProduct>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogProduct {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub price_centavos: i64,
    pub is_available: bool,
    pub variations: Vec<ProductVariation>,
    pub addon_groups: Vec<CatalogAddonGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogAddonGroup {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub min_select: i64,
    pub max_select: i64,
    pub options: Vec<AddonOption>,
}

impl CatalogAddonGroup {
    pub(crate) fn new(group: &AddonGroup, options: Vec<AddonOption>) -> Self {
        Self {
            id: group.id.clone(),
            name: group.name.clone(),
            name_en: group.name_en.clone(),
            name_es: group.name_es.clone(),
            min_select: group.min_select,
            max_select: group.max_select,
            options,
        }
    }
}

impl CatalogContent {
    pub(crate) fn build(menu: MenuSnapshot) -> Self {
        let categories = menu
            .categories
            .into_iter()
            .filter(|section| {
                section.category.is_visible
                    && section.products.iter().any(|item| item.product.is_visible)
            })
            .map(|section| {
                let cat = section.category;
                let products = section
                    .products
                    .into_iter()
                    .filter(|item| item.product.is_visible)
                    .map(|item| {
                        let product = item.product;
                        CatalogProduct {
                            variations: item.catalog_options.variations,
                            addon_groups: item.catalog_options.addon_groups,
                            id: product.id,
                            name: product.name,
                            name_en: product.name_en,
                            name_es: product.name_es,
                            description: product.description,
                            description_en: product.description_en,
                            description_es: product.description_es,
                            price_centavos: product.price_centavos,
                            is_available: product.is_available,
                        }
                    })
                    .collect();
                CatalogCategory {
                    id: cat.id,
                    name: cat.name,
                    name_en: cat.name_en,
                    name_es: cat.name_es,
                    description: cat.description,
                    description_en: cat.description_en,
                    description_es: cat.description_es,
                    products,
                }
            })
            .collect();
        Self {
            schema_version: SCHEMA_VERSION,
            categories,
        }
    }

    pub fn product(&self, product_id: &str) -> Option<&CatalogProduct> {
        self.categories
            .iter()
            .flat_map(|category| &category.products)
            .find(|product| product.id == product_id)
    }

    pub(crate) fn set_availability(
        &mut self,
        target: AvailabilityTarget<'_>,
        is_available: bool,
    ) -> AppResult<bool> {
        let mut products = self
            .categories
            .iter_mut()
            .flat_map(|category| &mut category.products);
        let not_found = AppError::NotFound("error.not_found");
        let availability = match target {
            AvailabilityTarget::Product(id) => products
                .find(|product| product.id == id)
                .map(|product| &mut product.is_available),
            AvailabilityTarget::Variation {
                product_id,
                variation_id,
            } => products
                .find(|product| product.id == product_id)
                .and_then(|product| product.variations.iter_mut().find(|v| v.id == variation_id))
                .map(|variation| &mut variation.is_available),
            AvailabilityTarget::AddonOption {
                group_id,
                option_id,
            } => {
                // The snapshot embeds reusable groups separately for every linked product.
                let options = products
                    .flat_map(|product| &mut product.addon_groups)
                    .filter(|group| group.id == group_id)
                    .flat_map(|group| &mut group.options)
                    .filter(|option| option.id == option_id);
                let mut found = false;
                let mut changed = false;
                for option in options {
                    found = true;
                    changed |=
                        std::mem::replace(&mut option.is_available, is_available) != is_available;
                }
                return if found { Ok(changed) } else { Err(not_found) };
            }
        };
        let availability = availability.ok_or(not_found)?;
        Ok(std::mem::replace(availability, is_available) != is_available)
    }

    pub(crate) fn canonical(&self) -> AppResult<(String, String)> {
        let mut value = serde_json::to_value(self).map_err(|error| {
            AppError::Internal(format!("catalog serialization failed: {error}"))
        })?;
        value.sort_all_objects();
        let json = value.to_string();
        let content_hash = Self::hash(&json);
        Ok((json, content_hash))
    }

    pub(crate) fn decode(json: &str, expected_hash: &str) -> AppResult<Self> {
        if Self::hash(json) != expected_hash {
            return Err(AppError::Internal("catalog snapshot hash mismatch".into()));
        }
        let content: Self = serde_json::from_str(json)
            .map_err(|error| AppError::Internal(format!("invalid catalog snapshot: {error}")))?;
        if content.schema_version != SCHEMA_VERSION {
            return Err(AppError::Internal(
                "unsupported catalog snapshot version".into(),
            ));
        }
        Ok(content)
    }

    fn hash(json: &str) -> String {
        hex::encode(Sha256::digest(json.as_bytes()))
    }
}
