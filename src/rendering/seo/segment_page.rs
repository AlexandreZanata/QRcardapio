use askama::Template;

use super::{SegmentCardView, SeoNavContext};
use crate::rendering::PageMeta;
use crate::seo::segments::{SegmentContent, SegmentId};

pub(super) struct SegmentVisual {
    pub art: &'static str,
    pub sticker: &'static str,
}

impl SegmentVisual {
    pub(super) fn for_id(id: SegmentId) -> Self {
        let (art, sticker) = match id {
            SegmentId::Restaurant => ("restaurant", "trade-restaurant"),
            SegmentId::BurgerShop => ("burger", "trade-burger"),
            SegmentId::CoffeeShop => ("coffee", "trade-cafe"),
            SegmentId::SnackBar => ("snack", "trade-snack"),
            SegmentId::Pizzeria => ("pizzeria", "trade-pizza"),
            SegmentId::AcaiShop => ("acai", "trade-acai"),
            SegmentId::Bakery => ("bakery", "trade-bakery"),
            SegmentId::BarPub => ("bar", "trade-bar"),
        };
        Self { art, sticker }
    }
}

struct SampleMenuItem {
    name: &'static str,
    price: &'static str,
}

struct SampleMenuCategory {
    name: &'static str,
    items: Vec<SampleMenuItem>,
}

#[derive(Template)]
#[template(path = "seo/segment.html")]
pub struct SegmentPageTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
    pub seg: &'static SegmentContent,
    pub related_segments: Vec<SegmentCardView>,
}

impl SegmentPageTemplate {
    fn visual(&self) -> SegmentVisual {
        SegmentVisual::for_id(self.seg.id)
    }

    fn sample_menu(&self) -> [SampleMenuCategory; 3] {
        self.seg
            .blueprint_categories
            .map(|(name, items)| SampleMenuCategory {
                name,
                items: items
                    .iter()
                    .map(|item| {
                        let (name, price) = item.rsplit_once(" — ").unwrap_or((item, ""));
                        SampleMenuItem { name, price }
                    })
                    .collect(),
            })
    }
}
