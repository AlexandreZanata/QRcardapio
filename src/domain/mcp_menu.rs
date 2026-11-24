use serde::{Deserialize, Serialize};

use super::{category::Category, product::Product};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextPatch {
    pub name: Option<String>,
    pub name_en: Option<String>,
    pub name_es: Option<String>,
    pub description: Option<String>,
    pub description_en: Option<String>,
    pub description_es: Option<String>,
}

impl TextPatch {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.name_en.is_none()
            && self.name_es.is_none()
            && self.description.is_none()
            && self.description_en.is_none()
            && self.description_es.is_none()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryPatch {
    pub id: String,
    pub updated_at: String,
    #[serde(default)]
    pub text: TextPatch,
    pub visible: Option<bool>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductPatch {
    pub id: String,
    pub updated_at: String,
    pub category_id: Option<String>,
    #[serde(default)]
    pub text: TextPatch,
    pub visible: Option<bool>,
    pub available: Option<bool>,
    pub price_centavos: Option<i64>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteRef {
    pub id: String,
    pub updated_at: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductOrderPatch {
    pub category_id: String,
    pub product_ids: Vec<String>,
}

#[derive(Default)]
pub struct ValidatedChanges {
    pub categories: Vec<Category>,
    pub products: Vec<Product>,
    pub category_updates: Vec<CategoryPatch>,
    pub product_updates: Vec<ProductPatch>,
    pub delete_categories: Vec<DeleteRef>,
    pub delete_products: Vec<DeleteRef>,
    pub category_order: Option<Vec<String>>,
    pub product_orders: Vec<ProductOrderPatch>,
}

#[derive(Deserialize, Serialize)]
pub struct AppliedChanges {
    pub category_ids: Vec<String>,
    pub product_ids: Vec<String>,
    pub categories_updated: usize,
    pub products_updated: usize,
    #[serde(default)]
    pub categories_deleted: usize,
    #[serde(default)]
    pub products_deleted: usize,
    #[serde(default)]
    pub categories_reordered: bool,
    #[serde(default)]
    pub products_reordered: usize,
}
