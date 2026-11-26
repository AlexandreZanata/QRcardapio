use askama::Template;

use super::catalog_options::CatalogOptionsView;
use crate::domain::category::Category;
use crate::domain::product::Product;
use crate::domain::publication::{MenuItemChoice, MenuSnapshot, SnapshotCategory, SnapshotProduct};
use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale, format_price};

struct ProductPhoto {
    url: String,
    width: i64,
    height: i64,
}

struct ChoiceView {
    name: (&'static str, &'static str, &'static str),
    display_name: &'static str,
    price: Option<(i64, String)>,
}

#[derive(Template)]
#[template(path = "menu/product_page.html")]
struct ProductPageTemplate<'a> {
    locale: Locale,
    catalog: &'static Catalog,
    establishment_name: &'a str,
    category: &'a Category,
    product: &'a Product,
    photo: Option<ProductPhoto>,
    catalog_options: CatalogOptionsView<'a>,
    removals: Vec<ChoiceView>,
    extras: Vec<ChoiceView>,
    price: String,
    back_href: String,
    theme_css_href: &'a str,
    is_preview: bool,
}

impl ProductPageTemplate<'_> {
    fn t(&self, key: &'static str) -> &'static str {
        self.catalog.t(self.locale, key)
    }

    fn title(&self, locale_code: &str) -> String {
        let locale = Locale::parse(locale_code).unwrap_or(self.locale);
        format!(
            "{} — {}",
            self.product.effective_name(locale),
            self.establishment_name
        )
    }

    fn product_name(&self) -> &str {
        self.product.effective_name(self.locale)
    }

    fn description(&self) -> &str {
        self.product.effective_description(self.locale)
    }

    fn category_name(&self) -> &str {
        self.category.effective_name(self.locale)
    }
}

pub fn render_product_detail_html(
    snapshot: &MenuSnapshot,
    category: &SnapshotCategory,
    item: &SnapshotProduct,
    locale: Locale,
    is_preview: bool,
    media_base_url: &str,
    theme_css_href: &str,
) -> AppResult<String> {
    let establishment = &snapshot.establishment;
    let back_href = if is_preview {
        format!("/dashboard/preview#cat-{}", category.category.id)
    } else {
        format!("/c/{}#cat-{}", establishment.slug, category.category.id)
    };
    let (extras, removals) = localized_choices(&item.choices, locale);
    let tpl = ProductPageTemplate {
        locale,
        catalog: Catalog::global(),
        establishment_name: &establishment.name,
        category: &category.category,
        product: &item.product,
        photo: item.image.as_ref().map(|image| ProductPhoto {
            url: format!("{media_base_url}/{}", image.filename_large),
            width: image.width,
            height: image.height,
        }),
        catalog_options: CatalogOptionsView::new(&item.catalog_options, locale),
        removals,
        extras,
        price: format_price(item.product.price_centavos, locale),
        back_href,
        theme_css_href,
        is_preview,
    };
    let html = tpl
        .render()
        .map_err(|error| AppError::Internal(format!("product detail render failed: {error}")))?;
    Ok(super::super::assets::version_static_assets(html))
}

fn localized_choices(
    choices: &[MenuItemChoice],
    locale: Locale,
) -> (Vec<ChoiceView>, Vec<ChoiceView>) {
    choices
        .iter()
        .map(|choice| ChoiceView {
            name: choice.name,
            display_name: match locale {
                Locale::PtBr => choice.name.0,
                Locale::En => choice.name.1,
                Locale::Es => choice.name.2,
            },
            price: choice
                .price_centavos
                .map(|price| (price, format_price(price, locale))),
        })
        .partition(|choice| choice.price.is_some())
}
