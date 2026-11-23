use axum::extract::Multipart;

use crate::error::{AppError, AppResult};
use crate::infrastructure::assets::MAX_PRODUCT_UPLOAD_BYTES;

#[derive(Default)]
pub(super) struct ParsedProductMultipart {
    pub csrf: String,
    pub category_id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub price: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub is_available: bool,
    pub is_visible: bool,
    pub remove_image: bool,
    pub image_bytes: Option<Vec<u8>>,
}

pub(super) async fn parse_product_multipart(
    mut multipart: Multipart,
) -> AppResult<ParsedProductMultipart> {
    let mut out = ParsedProductMultipart::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::Validation("error.invalid_image"))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "image" {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::Validation("error.image_too_large"))?;
            if bytes.len() > MAX_PRODUCT_UPLOAD_BYTES {
                return Err(AppError::Validation("error.image_too_large"));
            }
            if !bytes.is_empty() {
                out.image_bytes = Some(bytes.to_vec());
            }
        } else {
            let text = field
                .text()
                .await
                .map_err(|_| AppError::Validation("error.internal"))?;
            match name.as_str() {
                "_csrf" => out.csrf = text,
                "category_id" => out.category_id = text,
                "name" => out.name = text,
                "name_en" => out.name_en = text,
                "name_es" => out.name_es = text,
                "price" => out.price = text,
                "description" => out.description = text,
                "description_en" => out.description_en = text,
                "description_es" => out.description_es = text,
                "is_available" => out.is_available = text == "1" || text == "on",
                "is_visible" => out.is_visible = text == "1" || text == "on",
                "remove_image" => out.remove_image = text == "1" || text == "on",
                _ => {}
            }
        }
    }
    Ok(out)
}
