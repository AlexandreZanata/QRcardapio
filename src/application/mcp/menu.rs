use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::{
    category::Category,
    establishment::{sanitize_optional_text, validate_non_empty_name},
    mcp::is_url_token,
    mcp_menu::{AppliedChanges, CategoryPatch, ProductPatch, TextPatch, ValidatedChanges},
    product::{MAX_PRICE_CENTAVOS, Product},
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    find_category_for_est, find_establishment_by_id, mcp_menu as queries, mcp_oauth::Grant,
};
use crate::infrastructure::security::sha256_hex;

pub const CHANGE_LIMIT: usize = 80;
const PAGE_LIMIT: i64 = 40;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadMenu {
    pub category_id: Option<String>,
    #[serde(default)]
    pub offset: i64,
    pub limit: Option<i64>,
}

pub async fn read(
    pool: &SqlitePool,
    grant: &Grant,
    input: ReadMenu,
    origin: &str,
) -> AppResult<Value> {
    let limit = input.limit.unwrap_or(PAGE_LIMIT);
    if !(0..=100_000).contains(&input.offset) || !(1..=PAGE_LIMIT).contains(&limit) {
        return Err(invalid_changes());
    }
    let est = &grant.establishment_id;
    let record = find_establishment_by_id(pool, est)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let (key, mut entries) = if let Some(category) = input.category_id {
        let category = uuid(&category)?;
        find_category_for_est(pool, est, &category)
            .await?
            .ok_or(AppError::NotFound("error.not_found"))?;
        let products = queries::products(pool, est, &category, input.offset, limit + 1).await?;
        ("products", products.into_iter().map(|product| json!({
            "id": product.id, "updated_at": product.updated_at, "category_id": product.category_id,
            "text": {"name": product.name, "name_en": product.name_en, "name_es": product.name_es,
                "description": product.description, "description_en": product.description_en, "description_es": product.description_es},
            "price_centavos": product.price_centavos, "available": product.is_available, "visible": product.is_visible
        })).collect::<Vec<_>>())
    } else {
        let categories = queries::categories(pool, est, input.offset, limit + 1).await?;
        ("categories", categories.into_iter().map(|category| json!({
            "id": category.id, "updated_at": category.updated_at, "visible": category.is_visible,
            "text": {"name": category.name, "name_en": category.name_en, "name_es": category.name_es,
                "description": category.description, "description_en": category.description_en, "description_es": category.description_es}
        })).collect())
    };
    let next = (entries.len() > limit as usize).then_some(input.offset + limit);
    entries.truncate(limit as usize);
    let mut response = json!({"establishment": record.name, "next_offset": next, "draft": true, "preview_url": format!("{origin}/dashboard/preview")});
    response[key] = Value::Array(entries);
    Ok(response)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WriteMenu {
    operation_key: String,
    #[serde(default)]
    new_categories: Vec<NewCategory>,
    #[serde(default)]
    add_products: Vec<AppendProducts>,
    #[serde(default)]
    category_updates: Vec<CategoryPatch>,
    #[serde(default)]
    product_updates: Vec<ProductPatch>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NewCategory {
    text: TextPatch,
    visible: Option<bool>,
    #[serde(default)]
    products: Vec<NewProduct>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AppendProducts {
    category_id: String,
    products: Vec<NewProduct>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NewProduct {
    text: TextPatch,
    price_centavos: i64,
    visible: Option<bool>,
    available: Option<bool>,
}

pub async fn apply(
    pool: &SqlitePool,
    grant: &Grant,
    input: WriteMenu,
) -> AppResult<AppliedChanges> {
    let count = input
        .new_categories
        .iter()
        .map(|entry| 1 + entry.products.len())
        .sum::<usize>()
        + input
            .add_products
            .iter()
            .map(|entry| entry.products.len())
            .sum::<usize>()
        + input.category_updates.len()
        + input.product_updates.len();
    if !(1..=CHANGE_LIMIT).contains(&count)
        || input
            .add_products
            .iter()
            .any(|group| group.products.is_empty())
        || !is_url_token(&input.operation_key, 4, 96)
    {
        return Err(invalid_changes());
    }
    let fingerprint = sha256_hex(&serde_json::to_string(&input).map_err(|_| invalid_changes())?);
    let now = Utc::now();
    let timestamp = now.to_rfc3339();
    let est = &grant.establishment_id;
    let mut changes = ValidatedChanges::default();
    for entry in input.new_categories {
        let text = validate_text(entry.text, 80, 240)?;
        let id = Uuid::now_v7().to_string();
        changes
            .products
            .extend(build_products(est, &id, entry.products, &timestamp)?);
        changes.categories.push(Category {
            id,
            establishment_id: est.clone(),
            name: text.name.ok_or_else(invalid_changes)?,
            name_en: text.name_en.unwrap_or_default(),
            name_es: text.name_es.unwrap_or_default(),
            description: text.description.unwrap_or_default(),
            description_en: text.description_en.unwrap_or_default(),
            description_es: text.description_es.unwrap_or_default(),
            position: 0,
            is_visible: entry.visible.unwrap_or(true),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        });
    }
    for entry in input.add_products {
        changes.products.extend(build_products(
            est,
            &uuid(&entry.category_id)?,
            entry.products,
            &timestamp,
        )?);
    }
    for mut patch in input.category_updates {
        patch.id = uuid(&patch.id)?;
        validate_version(&patch.updated_at)?;
        if patch.text.is_empty() && patch.visible.is_none() {
            return Err(invalid_changes());
        }
        patch.text = validate_text(patch.text, 80, 240)?;
        changes.category_updates.push(patch);
    }
    for mut patch in input.product_updates {
        patch.id = uuid(&patch.id)?;
        validate_version(&patch.updated_at)?;
        if patch.text.is_empty()
            && patch.visible.is_none()
            && patch.available.is_none()
            && patch.price_centavos.is_none()
        {
            return Err(invalid_changes());
        }
        patch.price_centavos.map(price).transpose()?;
        patch.text = validate_text(patch.text, 100, 320)?;
        changes.product_updates.push(patch);
    }
    queries::apply(
        pool,
        grant,
        queries::Operation {
            key: &input.operation_key,
            hash: &fingerprint,
            now: now.timestamp(),
            timestamp: &timestamp,
        },
        changes,
    )
    .await
}

fn build_products(
    est: &str,
    category: &str,
    items: Vec<NewProduct>,
    now: &str,
) -> AppResult<Vec<Product>> {
    items
        .into_iter()
        .map(|item| {
            let text = validate_text(item.text, 100, 320)?;
            Ok(Product {
                id: Uuid::now_v7().to_string(),
                establishment_id: est.to_string(),
                category_id: category.to_string(),
                name: text.name.ok_or_else(invalid_changes)?,
                name_en: text.name_en.unwrap_or_default(),
                name_es: text.name_es.unwrap_or_default(),
                description: text.description.unwrap_or_default(),
                description_en: text.description_en.unwrap_or_default(),
                description_es: text.description_es.unwrap_or_default(),
                price_centavos: price(item.price_centavos)?,
                image_asset_id: None,
                position: 0,
                is_available: item.available.unwrap_or(true),
                is_visible: item.visible.unwrap_or(true),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            })
        })
        .collect()
}

fn uuid(input: &str) -> AppResult<String> {
    Uuid::parse_str(input)
        .map(|id| id.to_string())
        .map_err(|_| invalid_changes())
}

fn validate_version(input: &str) -> AppResult<()> {
    DateTime::parse_from_rfc3339(input)
        .map(|_| ())
        .map_err(|_| invalid_changes())
}

fn price(value: i64) -> AppResult<i64> {
    if (0..=MAX_PRICE_CENTAVOS).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::Validation("error.invalid_price"))
    }
}

fn validate_text(mut text: TextPatch, names: usize, descriptions: usize) -> AppResult<TextPatch> {
    text.name = text
        .name
        .map(|name| validate_non_empty_name(&name, names))
        .transpose()?;
    for value in [&mut text.name_en, &mut text.name_es] {
        *value = value
            .take()
            .map(|value| sanitize_optional_text(&value, names));
    }
    for value in [
        &mut text.description,
        &mut text.description_en,
        &mut text.description_es,
    ] {
        *value = value
            .take()
            .map(|value| sanitize_optional_text(&value, descriptions));
    }
    Ok(text)
}

fn invalid_changes() -> AppError {
    AppError::Validation("mcp.invalid_changes")
}
