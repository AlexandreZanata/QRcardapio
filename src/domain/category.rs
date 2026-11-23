use crate::i18n::{Locale, pick_localized_text};

#[derive(Debug, Clone)]
pub struct Category {
    pub id: String,
    pub establishment_id: String,
    pub name: String,
    pub name_en: String,
    pub name_es: String,
    pub description: String,
    pub description_en: String,
    pub description_es: String,
    pub position: i64,
    pub is_visible: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Category {
    pub fn effective_name(&self, locale: Locale) -> &str {
        pick_localized_text(locale, &self.name, &self.name_en, &self.name_es)
    }

    pub fn effective_description(&self, locale: Locale) -> &str {
        pick_localized_text(
            locale,
            &self.description,
            &self.description_en,
            &self.description_es,
        )
    }
}
