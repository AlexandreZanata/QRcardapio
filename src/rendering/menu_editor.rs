use askama::Template;

use super::PageMeta;

pub struct EditorProductView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price_centavos: i64,
    pub price_display: String,
    pub thumb_url: Option<String>,
    pub is_available: bool,
    pub is_visible: bool,
}

pub struct EditorCategoryView {
    pub id: String,
    pub name: String,
    pub is_visible: bool,
    pub products: Vec<EditorProductView>,
}

pub struct CategoryOptionView {
    pub id: String,
    pub name: String,
}

#[derive(Template)]
#[template(path = "dashboard/menu.html")]
pub struct MenuEditorTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub has_unpublished_changes: bool,
    pub categories: Vec<EditorCategoryView>,
}

#[derive(Template)]
#[template(path = "dashboard/category_form.html")]
pub struct CategoryFormTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub form_action: String,
    pub name_value: String,
    pub name_en: String,
    pub name_es: String,
    pub description_value: String,
    pub description_en: String,
    pub description_es: String,
    pub is_visible: bool,
    pub error_key: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard/product_form.html")]
pub struct ProductFormTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub form_action: String,
    pub category_options: Vec<CategoryOptionView>,
    pub selected_category_id: String,
    pub name_value: String,
    pub name_en: String,
    pub name_es: String,
    pub price_value: String,
    pub description_value: String,
    pub description_en: String,
    pub description_es: String,
    pub existing_image_url: Option<String>,
    pub is_available: bool,
    pub is_visible: bool,
    pub error_key: Option<String>,
}
