use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVariation {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub price_centavos: i64,
    pub position: i64,
    pub is_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonGroup {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub min_select: i64,
    pub max_select: i64,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonOption {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub price_centavos: i64,
    pub position: i64,
    pub max_quantity: i64,
    pub is_available: bool,
}
