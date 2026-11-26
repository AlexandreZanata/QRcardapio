use crate::i18n::Locale;

use super::segments::SegmentId;
use super::tools::ToolId;

#[path = "guides_en.rs"]
mod guides_en;
#[path = "guides_es.rs"]
mod guides_es;
#[path = "guides_pt.rs"]
mod guides_pt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideId {
    CreateQrMenu,
    PdfVsDigital,
    GoogleMapsReviews,
    MenuEngineeringQr,
}

impl GuideId {
    pub const ALL: [GuideId; 4] = [
        Self::CreateQrMenu,
        Self::PdfVsDigital,
        Self::GoogleMapsReviews,
        Self::MenuEngineeringQr,
    ];

    pub const fn art(self) -> &'static str {
        match self {
            Self::CreateQrMenu => "create-menu",
            Self::PdfVsDigital => "pdf-vs-digital",
            Self::GoogleMapsReviews => "maps-reviews",
            Self::MenuEngineeringQr => "menu-engineering",
        }
    }

    pub const fn slug(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::CreateQrMenu, Locale::PtBr) => "como-criar-cardapio-digital-qr-code",
            (Self::CreateQrMenu, Locale::En) => "how-to-create-qr-code-digital-menu",
            (Self::CreateQrMenu, Locale::Es) => "como-crear-menu-digital-qr",
            (Self::PdfVsDigital, Locale::PtBr) => "cardapio-pdf-vs-cardapio-digital",
            (Self::PdfVsDigital, Locale::En) => "pdf-menu-vs-digital-menu",
            (Self::PdfVsDigital, Locale::Es) => "menu-pdf-vs-menu-digital",
            (Self::GoogleMapsReviews, Locale::PtBr) => "mais-avaliacoes-google-maps-restaurante",
            (Self::GoogleMapsReviews, Locale::En) => "more-google-maps-reviews-restaurant",
            (Self::GoogleMapsReviews, Locale::Es) => "mas-resenas-google-maps-restaurante",
            (Self::MenuEngineeringQr, Locale::PtBr) => "engenharia-cardapio-qr-code-impressao",
            (Self::MenuEngineeringQr, Locale::En) => "menu-engineering-qr-code-printing",
            (Self::MenuEngineeringQr, Locale::Es) => "ingenieria-menu-impresion-qr",
        }
    }

    pub const fn full_path(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::CreateQrMenu, Locale::PtBr) => "/guias/como-criar-cardapio-digital-qr-code",
            (Self::CreateQrMenu, Locale::En) => "/en/guides/how-to-create-qr-code-digital-menu",
            (Self::CreateQrMenu, Locale::Es) => "/es/guias/como-crear-menu-digital-qr",
            (Self::PdfVsDigital, Locale::PtBr) => "/guias/cardapio-pdf-vs-cardapio-digital",
            (Self::PdfVsDigital, Locale::En) => "/en/guides/pdf-menu-vs-digital-menu",
            (Self::PdfVsDigital, Locale::Es) => "/es/guias/menu-pdf-vs-menu-digital",
            (Self::GoogleMapsReviews, Locale::PtBr) => {
                "/guias/mais-avaliacoes-google-maps-restaurante"
            }
            (Self::GoogleMapsReviews, Locale::En) => {
                "/en/guides/more-google-maps-reviews-restaurant"
            }
            (Self::GoogleMapsReviews, Locale::Es) => {
                "/es/guias/mas-resenas-google-maps-restaurante"
            }
            (Self::MenuEngineeringQr, Locale::PtBr) => {
                "/guias/engenharia-cardapio-qr-code-impressao"
            }
            (Self::MenuEngineeringQr, Locale::En) => "/en/guides/menu-engineering-qr-code-printing",
            (Self::MenuEngineeringQr, Locale::Es) => "/es/guias/ingenieria-menu-impresion-qr",
        }
    }

    pub fn from_slug(locale: Locale, slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.slug(locale) == slug)
    }

    pub fn content(self, locale: Locale) -> &'static GuideContent {
        match locale {
            Locale::PtBr => guides_pt::get(self),
            Locale::En => guides_en::get(self),
            Locale::Es => guides_es::get(self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GuideSection {
    pub heading: &'static str,
    pub paragraphs: &'static [&'static str],
    pub bullets: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub struct ComparisonTable {
    pub title: &'static str,
    pub headers: [&'static str; 3],
    pub rows: &'static [[&'static str; 3]],
}

#[derive(Debug, Clone, Copy)]
pub struct GuideContent {
    pub id: GuideId,
    pub title: &'static str,
    pub short_desc: &'static str,
    pub meta_title: &'static str,
    pub meta_description: &'static str,
    pub read_minutes: u8,
    pub published_date: &'static str,
    pub answer_summary: &'static str,
    pub sections: &'static [GuideSection],
    pub comparison_table: Option<ComparisonTable>,
    pub faqs: [(&'static str, &'static str); 3],
    pub related_segment: SegmentId,
    pub related_tool: ToolId,
}

pub fn all_guides(locale: Locale) -> Vec<&'static GuideContent> {
    GuideId::ALL
        .into_iter()
        .map(|id| id.content(locale))
        .collect()
}
