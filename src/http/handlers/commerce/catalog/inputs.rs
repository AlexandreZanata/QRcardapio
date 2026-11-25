use serde::Deserialize;
use uuid::Uuid;

use crate::domain::commerce::catalog_options::{AddonGroup, AddonOption, ProductVariation};
use crate::domain::commerce::pricing::MAX_ORDER_TOTAL_CENTAVOS;
use crate::domain::product::parse_price_centavos;
use crate::error::{AppError, AppResult};

const INVALID: AppError = AppError::Validation("commerce.catalog.invalid_options");

#[derive(Deserialize)]
pub struct OptionForm {
    pub _csrf: String,
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_en: String,
    #[serde(default)]
    pub name_es: String,
    pub position: i64,
    pub price: Option<String>,
    pub min_select: Option<i64>,
    pub max_select: Option<i64>,
    pub max_quantity: Option<i64>,
    pub available: Option<String>,
}

impl OptionForm {
    fn validate(&mut self) -> AppResult<()> {
        self.name = self.name.trim().to_owned();
        self.name_en = self.name_en.trim().to_owned();
        self.name_es = self.name_es.trim().to_owned();
        if self.name.is_empty()
            || [&self.name, &self.name_en, &self.name_es]
                .iter()
                .any(|name| name.chars().count() > 100 || name.chars().any(char::is_control))
            || !(0..=1_000_000).contains(&self.position)
        {
            return Err(INVALID);
        }
        self.id = if self.id.is_empty() {
            Uuid::now_v7().to_string()
        } else {
            Uuid::parse_str(&self.id).map_err(|_| INVALID)?.to_string()
        };
        Ok(())
    }

    fn price(&self) -> AppResult<i64> {
        let value = parse_price_centavos(self.price.as_deref().ok_or(INVALID)?)?;
        if value > MAX_ORDER_TOTAL_CENTAVOS {
            return Err(INVALID);
        }
        Ok(value)
    }

    pub fn variation(mut self) -> AppResult<ProductVariation> {
        self.validate()?;
        let price_centavos = self.price()?;
        Ok(ProductVariation {
            id: self.id,
            name: self.name,
            name_en: self.name_en,
            name_es: self.name_es,
            price_centavos,
            position: self.position,
            is_available: self.available.is_some(),
        })
    }

    pub fn group(mut self) -> AppResult<AddonGroup> {
        self.validate()?;
        let min_select = self.min_select.ok_or(INVALID)?;
        let max_select = self.max_select.ok_or(INVALID)?;
        if !(0..=max_select).contains(&min_select) || max_select > 20 {
            return Err(INVALID);
        }
        Ok(AddonGroup {
            id: self.id,
            name: self.name,
            name_en: self.name_en,
            name_es: self.name_es,
            min_select,
            max_select,
            position: self.position,
        })
    }

    pub fn addon(mut self) -> AppResult<AddonOption> {
        self.validate()?;
        let price_centavos = self.price()?;
        let max_quantity = self
            .max_quantity
            .filter(|qty| (1..=99).contains(qty))
            .ok_or(INVALID)?;
        Ok(AddonOption {
            id: self.id,
            name: self.name,
            name_en: self.name_en,
            name_es: self.name_es,
            price_centavos,
            position: self.position,
            max_quantity,
            is_available: self.available.is_some(),
        })
    }
}
