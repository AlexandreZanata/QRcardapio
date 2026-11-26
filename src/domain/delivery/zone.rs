use serde::Serialize;

use crate::domain::commerce::pricing::MAX_ORDER_TOTAL_CENTAVOS;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryZone {
    pub id: String,
    pub establishment_id: String,
    pub neighborhood: String,
    pub city: String,
    pub normalized_key: String,
    pub fee_centavos: i64,
    pub min_order_centavos: Option<i64>,
    pub free_delivery_from_centavos: Option<i64>,
    pub eta_min_minutes: i64,
    pub eta_max_minutes: i64,
    pub is_active: bool,
    pub position: i64,
}

impl DeliveryZone {
    /// Delivery fee for `items_subtotal_centavos`, returning 0 when the free-delivery
    /// threshold is configured and met.
    pub fn effective_fee_centavos(&self, items_subtotal_centavos: i64) -> i64 {
        if self
            .free_delivery_from_centavos
            .is_some_and(|threshold| items_subtotal_centavos >= threshold)
        {
            0
        } else {
            self.fee_centavos
        }
    }

    pub fn effective_min_order_centavos(&self, global_minimum_centavos: i64) -> i64 {
        self.min_order_centavos.unwrap_or(global_minimum_centavos)
    }
}

pub struct DeliveryZoneInput<'a> {
    pub neighborhood: &'a str,
    pub city: &'a str,
    pub fee_centavos: i64,
    pub min_order_centavos: Option<i64>,
    pub free_delivery_from_centavos: Option<i64>,
    pub eta_min_minutes: i64,
    pub eta_max_minutes: i64,
    pub is_active: bool,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedDeliveryZone {
    pub neighborhood: String,
    pub city: String,
    pub normalized_key: String,
    pub fee_centavos: i64,
    pub min_order_centavos: Option<i64>,
    pub free_delivery_from_centavos: Option<i64>,
    pub eta_min_minutes: i64,
    pub eta_max_minutes: i64,
    pub is_active: bool,
    pub position: i64,
}

impl ValidatedDeliveryZone {
    pub fn validate(input: DeliveryZoneInput<'_>) -> AppResult<Self> {
        const INVALID: AppError = AppError::Validation("delivery.invalid_zone");
        let neighborhood = clean_label(input.neighborhood, 80)?;
        let city = clean_label(input.city, 80)?;
        let normalized_key = normalize_neighborhood_key(&neighborhood, &city);
        if normalized_key.is_empty() {
            return Err(INVALID);
        }
        if !(0..=MAX_ORDER_TOTAL_CENTAVOS).contains(&input.fee_centavos) {
            return Err(INVALID);
        }
        if let Some(min) = input.min_order_centavos
            && !(0..=MAX_ORDER_TOTAL_CENTAVOS).contains(&min)
        {
            return Err(INVALID);
        }
        if let Some(free_from) = input.free_delivery_from_centavos
            && !(1..=MAX_ORDER_TOTAL_CENTAVOS).contains(&free_from)
        {
            return Err(INVALID);
        }
        if !(1..=240).contains(&input.eta_min_minutes)
            || !(input.eta_min_minutes..=360).contains(&input.eta_max_minutes)
            || input.position < 0
        {
            return Err(INVALID);
        }
        Ok(Self {
            neighborhood,
            city,
            normalized_key,
            fee_centavos: input.fee_centavos,
            min_order_centavos: input.min_order_centavos,
            free_delivery_from_centavos: input.free_delivery_from_centavos,
            eta_min_minutes: input.eta_min_minutes,
            eta_max_minutes: input.eta_max_minutes,
            is_active: input.is_active,
            position: input.position,
        })
    }
}

fn clean_label(raw: &str, max_len: usize) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > max_len
        || trimmed.chars().any(char::is_control)
    {
        return Err(AppError::Validation("delivery.invalid_zone"));
    }
    Ok(trimmed.to_owned())
}

pub fn normalize_neighborhood_key(neighborhood: &str, city: &str) -> String {
    format!("{}|{}", fold_ascii(neighborhood), fold_ascii(city))
}

fn fold_ascii(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_space = false;
    for ch in raw.trim().chars() {
        let folded = match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            'ñ' | 'Ñ' => 'n',
            other if other.is_ascii_alphanumeric() => other.to_ascii_lowercase(),
            _ => {
                if !out.is_empty() {
                    pending_space = true;
                }
                continue;
            }
        };
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(folded);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_neighborhood_keys_and_computes_free_delivery_threshold() {
        assert_eq!(
            normalize_neighborhood_key("Jardim São João", "Cuiabá"),
            normalize_neighborhood_key("jardim sao joao", "cuiaba")
        );
        let zone = DeliveryZone {
            id: "z1".into(),
            establishment_id: "e1".into(),
            neighborhood: "Centro".into(),
            city: "Cuiabá".into(),
            normalized_key: "centro|cuiaba".into(),
            fee_centavos: 800,
            min_order_centavos: Some(3000),
            free_delivery_from_centavos: Some(8000),
            eta_min_minutes: 25,
            eta_max_minutes: 45,
            is_active: true,
            position: 0,
        };
        assert_eq!(zone.effective_fee_centavos(7999), 800);
        assert_eq!(zone.effective_fee_centavos(8000), 0);
        assert_eq!(zone.effective_min_order_centavos(2000), 3000);
    }
}
