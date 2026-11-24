use crate::i18n::Locale;

#[path = "segments_en.rs"]
mod segments_en;
#[path = "segments_es.rs"]
mod segments_es;
#[path = "segments_pt.rs"]
mod segments_pt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentId {
    Restaurant,
    BurgerShop,
    CoffeeShop,
    SnackBar,
    Pizzeria,
    AcaiShop,
    Bakery,
    BarPub,
}

impl SegmentId {
    pub const ALL: [SegmentId; 8] = [
        Self::Restaurant,
        Self::BurgerShop,
        Self::CoffeeShop,
        Self::SnackBar,
        Self::Pizzeria,
        Self::AcaiShop,
        Self::Bakery,
        Self::BarPub,
    ];

    pub const fn slug(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Restaurant, Locale::PtBr) => "restaurante",
            (Self::Restaurant, Locale::En) => "restaurant",
            (Self::Restaurant, Locale::Es) => "restaurante",
            (Self::BurgerShop, Locale::PtBr) => "hamburgueria",
            (Self::BurgerShop, Locale::En) => "burger-shop",
            (Self::BurgerShop, Locale::Es) => "hamburgueseria",
            (Self::CoffeeShop, Locale::PtBr) => "cafeteria",
            (Self::CoffeeShop, Locale::En) => "coffee-shop",
            (Self::CoffeeShop, Locale::Es) => "cafeteria",
            (Self::SnackBar, Locale::PtBr) => "lanchonete",
            (Self::SnackBar, Locale::En) => "snack-bar",
            (Self::SnackBar, Locale::Es) => "cafeteria-rapida",
            (Self::Pizzeria, Locale::PtBr) => "pizzaria",
            (Self::Pizzeria, Locale::En) => "pizzeria",
            (Self::Pizzeria, Locale::Es) => "pizzeria",
            (Self::AcaiShop, Locale::PtBr) => "acaiteria-sorveteria",
            (Self::AcaiShop, Locale::En) => "acai-ice-cream-shop",
            (Self::AcaiShop, Locale::Es) => "heladeria-acai",
            (Self::Bakery, Locale::PtBr) => "padaria",
            (Self::Bakery, Locale::En) => "bakery",
            (Self::Bakery, Locale::Es) => "panaderia",
            (Self::BarPub, Locale::PtBr) => "bar-pub",
            (Self::BarPub, Locale::En) => "bar-pub",
            (Self::BarPub, Locale::Es) => "bar-pub",
        }
    }

    pub const fn full_path(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Restaurant, Locale::PtBr) => "/cardapio-digital/restaurante",
            (Self::Restaurant, Locale::En) => "/en/digital-menu/restaurant",
            (Self::Restaurant, Locale::Es) => "/es/menu-digital/restaurante",
            (Self::BurgerShop, Locale::PtBr) => "/cardapio-digital/hamburgueria",
            (Self::BurgerShop, Locale::En) => "/en/digital-menu/burger-shop",
            (Self::BurgerShop, Locale::Es) => "/es/menu-digital/hamburgueseria",
            (Self::CoffeeShop, Locale::PtBr) => "/cardapio-digital/cafeteria",
            (Self::CoffeeShop, Locale::En) => "/en/digital-menu/coffee-shop",
            (Self::CoffeeShop, Locale::Es) => "/es/menu-digital/cafeteria",
            (Self::SnackBar, Locale::PtBr) => "/cardapio-digital/lanchonete",
            (Self::SnackBar, Locale::En) => "/en/digital-menu/snack-bar",
            (Self::SnackBar, Locale::Es) => "/es/menu-digital/cafeteria-rapida",
            (Self::Pizzeria, Locale::PtBr) => "/cardapio-digital/pizzaria",
            (Self::Pizzeria, Locale::En) => "/en/digital-menu/pizzeria",
            (Self::Pizzeria, Locale::Es) => "/es/menu-digital/pizzeria",
            (Self::AcaiShop, Locale::PtBr) => "/cardapio-digital/acaiteria-sorveteria",
            (Self::AcaiShop, Locale::En) => "/en/digital-menu/acai-ice-cream-shop",
            (Self::AcaiShop, Locale::Es) => "/es/menu-digital/heladeria-acai",
            (Self::Bakery, Locale::PtBr) => "/cardapio-digital/padaria",
            (Self::Bakery, Locale::En) => "/en/digital-menu/bakery",
            (Self::Bakery, Locale::Es) => "/es/menu-digital/panaderia",
            (Self::BarPub, Locale::PtBr) => "/cardapio-digital/bar-pub",
            (Self::BarPub, Locale::En) => "/en/digital-menu/bar-pub",
            (Self::BarPub, Locale::Es) => "/es/menu-digital/bar-pub",
        }
    }

    pub fn from_slug(locale: Locale, slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.slug(locale) == slug)
    }

    pub fn content(self, locale: Locale) -> &'static SegmentContent {
        match locale {
            Locale::PtBr => segments_pt::get(self),
            Locale::En => segments_en::get(self),
            Locale::Es => segments_es::get(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentContent {
    pub id: SegmentId,
    pub name: &'static str,
    pub short_desc: &'static str,
    pub meta_title: &'static str,
    pub meta_description: &'static str,
    pub h1: &'static str,
    pub answer_summary: &'static str,
    pub pain_heading: &'static str,
    pub pain_points: [(&'static str, &'static str); 3],
    pub blueprint_title: &'static str,
    pub blueprint_categories: [(&'static str, &'static [&'static str]); 3],
    pub tips_heading: &'static str,
    pub operational_tips: [&'static str; 3],
    pub faqs: [(&'static str, &'static str); 3],
    pub related: [SegmentId; 3],
}

pub fn all_segments(locale: Locale) -> Vec<&'static SegmentContent> {
    SegmentId::ALL
        .into_iter()
        .map(|id| id.content(locale))
        .collect()
}
