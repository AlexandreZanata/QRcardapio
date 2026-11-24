use serde::{Deserialize, Deserializer, Serialize};

use crate::i18n::Locale;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSettingsInput {
    pub operation_key: String,
    pub updated_at: String,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub description_en: Option<String>,
    pub description_es: Option<String>,
    pub accent_color: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub google_maps_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub google_review_url: Option<Option<String>>,
    pub preferred_locale: Option<String>,
}

impl AccountSettingsInput {
    pub fn has_establishment_changes(&self) -> bool {
        self.name.is_some()
            || self.slug.is_some()
            || self.description.is_some()
            || self.description_en.is_some()
            || self.description_es.is_some()
            || self.accent_color.is_some()
            || self.google_maps_url.is_some()
            || self.google_review_url.is_some()
    }

    pub fn is_empty(&self) -> bool {
        !self.has_establishment_changes() && self.preferred_locale.is_none()
    }
}

#[derive(Debug)]
pub struct ValidatedAccountPatch {
    pub expected_updated_at: String,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub description_en: Option<String>,
    pub description_es: Option<String>,
    pub accent_color: Option<String>,
    pub google_maps_url: Option<Option<String>>,
    pub google_review_url: Option<Option<String>>,
    pub preferred_locale: Option<Locale>,
}

impl ValidatedAccountPatch {
    pub fn has_establishment_changes(&self) -> bool {
        self.name.is_some()
            || self.slug.is_some()
            || self.description.is_some()
            || self.description_en.is_some()
            || self.description_es.is_some()
            || self.accent_color.is_some()
            || self.google_maps_url.is_some()
            || self.google_review_url.is_some()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccountPatchOutcome {
    pub establishment_updated: bool,
    pub locale_updated: bool,
    pub slug: String,
    pub updated_at: String,
    pub preferred_locale: String,
}

fn deserialize_nullable<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}
