use crate::domain::publication::ProductCatalogOptions;
use crate::i18n::{Locale, format_price, pick_localized_text};

pub struct LocalizedOptionText<'a> {
    pub pt: &'a str,
    pub en: &'a str,
    pub es: &'a str,
    pub display: &'a str,
}

impl<'a> LocalizedOptionText<'a> {
    fn new(locale: Locale, pt: &'a str, en: &'a str, es: &'a str) -> Self {
        Self {
            pt,
            en,
            es,
            display: pick_localized_text(locale, pt, en, es),
        }
    }
}

pub struct CatalogChoice<'a> {
    pub name: LocalizedOptionText<'a>,
    pub price_centavos: i64,
    pub price_display: String,
}

impl<'a> CatalogChoice<'a> {
    fn new(locale: Locale, pt: &'a str, en: &'a str, es: &'a str, price_centavos: i64) -> Self {
        Self {
            name: LocalizedOptionText::new(locale, pt, en, es),
            price_centavos,
            price_display: format_price(price_centavos, locale),
        }
    }
}

pub struct CatalogAddonGroupView<'a> {
    pub name: LocalizedOptionText<'a>,
    pub options: Vec<CatalogChoice<'a>>,
}

pub struct CatalogOptionsView<'a> {
    pub variations: Vec<CatalogChoice<'a>>,
    pub addon_groups: Vec<CatalogAddonGroupView<'a>>,
}

impl<'a> CatalogOptionsView<'a> {
    pub(super) fn new(options: &'a ProductCatalogOptions, locale: Locale) -> Self {
        let variations = options
            .variations
            .iter()
            .filter(|variation| variation.is_available)
            .map(|variation| {
                CatalogChoice::new(
                    locale,
                    &variation.name,
                    &variation.name_en,
                    &variation.name_es,
                    variation.price_centavos,
                )
            })
            .collect();
        let addon_groups = options
            .addon_groups
            .iter()
            .filter_map(|group| {
                let options: Vec<_> = group
                    .options
                    .iter()
                    .filter(|option| option.is_available)
                    .map(|option| {
                        CatalogChoice::new(
                            locale,
                            &option.name,
                            &option.name_en,
                            &option.name_es,
                            option.price_centavos,
                        )
                    })
                    .collect();
                (!options.is_empty()).then(|| CatalogAddonGroupView {
                    name: LocalizedOptionText::new(
                        locale,
                        &group.name,
                        &group.name_en,
                        &group.name_es,
                    ),
                    options,
                })
            })
            .collect();
        Self {
            variations,
            addon_groups,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.variations.is_empty() && self.addon_groups.is_empty()
    }
}
