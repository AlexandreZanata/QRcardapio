use serde_json::{Value, json};

use crate::domain::billing::BillingPlan;
use crate::domain::publication::MenuSnapshot;
use crate::i18n::{Catalog, Locale};

use super::faq::LANDING_FAQS;
use super::guides::GuideContent;
use super::segments::SegmentContent;
use super::tools::ToolContent;
use super::{absolute_url, format_decimal_price, normalized_base_url, og_image_url, safe_json_ld};

fn organization_node(base: &str) -> Value {
    json!({
        "@type": "Organization",
        "@id": format!("{base}/#organization"),
        "name": "QR Cardápio",
        "url": format!("{base}/"),
        "logo": {
            "@type": "ImageObject",
            "url": format!("{base}/static/icons/icon-512.png"),
            "width": 512,
            "height": 512
        },
        "areaServed": "BR",
        "availableLanguage": ["pt-BR", "en", "es"]
    })
}

fn website_node(base: &str) -> Value {
    json!({
        "@type": "WebSite",
        "@id": format!("{base}/#website"),
        "url": format!("{base}/"),
        "name": "QR Cardápio",
        "inLanguage": ["pt-BR", "en", "es"],
        "publisher": { "@id": format!("{base}/#organization") }
    })
}

fn breadcrumb_node(canonical: &str, items: Vec<(&str, &str)>) -> Value {
    let list_items: Vec<Value> = items
        .into_iter()
        .enumerate()
        .map(|(idx, (name, url))| {
            json!({
                "@type": "ListItem",
                "position": idx + 1,
                "name": name,
                "item": url
            })
        })
        .collect();
    json!({
        "@type": "BreadcrumbList",
        "@id": format!("{canonical}#breadcrumb"),
        "itemListElement": list_items
    })
}

fn faq_node(canonical: &str, locale: Locale, pairs: &[(String, String)]) -> Value {
    let entities: Vec<Value> = pairs
        .iter()
        .map(|(q, a)| {
            json!({
                "@type": "Question",
                "name": q,
                "acceptedAnswer": {
                    "@type": "Answer",
                    "text": a
                }
            })
        })
        .collect();
    json!({
        "@type": "FAQPage",
        "@id": format!("{canonical}#faq"),
        "inLanguage": locale.code(),
        "mainEntity": entities
    })
}

pub fn landing_json_ld(
    base_url: &str,
    locale: Locale,
    canonical: &str,
    title: &str,
    description: &str,
    show_plan: bool,
    catalog: &Catalog,
) -> String {
    let base = normalized_base_url(base_url);
    let og_img = og_image_url(base, locale);

    let mut software = json!({
        "@type": "SoftwareApplication",
        "@id": format!("{base}/#software"),
        "name": "QR Cardápio",
        "description": description,
        "url": format!("{base}/"),
        "applicationCategory": "BusinessApplication",
        "operatingSystem": "Web, Android, iOS",
        "inLanguage": ["pt-BR", "en", "es"],
        "image": og_img,
        "screenshot": [
            format!("{base}/static/images/landing-dashboard-1536.webp"),
            format!("{base}/static/images/landing-menu-1080.webp")
        ],
        "featureList": [
            catalog.t(locale, "landing.plan_feature_menu"),
            catalog.t(locale, "landing.plan_feature_qr"),
            catalog.t(locale, "landing.plan_feature_updates"),
            catalog.t(locale, "landing.plan_feature_style"),
            catalog.t(locale, "landing.plan_feature_reviews")
        ],
        "publisher": { "@id": format!("{base}/#organization") }
    });
    if show_plan {
        let offers: Vec<Value> = BillingPlan::ALL
            .into_iter()
            .map(|plan| {
                let name_key = match plan {
                    BillingPlan::Monthly => "billing.monthly",
                    BillingPlan::Semiannual => "billing.semiannual",
                    BillingPlan::Annual => "billing.annual",
                };
                json!({
                    "@type": "Offer",
                    "name": format!("{} — {}", catalog.t(locale, "landing.plan_name"), catalog.t(locale, name_key)),
                    "price": format_decimal_price(plan.price()),
                    "priceCurrency": "BRL",
                    "availability": "https://schema.org/InStock",
                    "url": format!("{canonical}#plan")
                })
            })
            .collect();
        software["offers"] = Value::Array(offers);
    }

    let howto = json!({
        "@type": "HowTo",
        "@id": format!("{canonical}#howto"),
        "name": catalog.t(locale, "landing.how_title"),
        "description": catalog.t(locale, "landing.how_subtitle"),
        "inLanguage": locale.code(),
        "step": [
            {
                "@type": "HowToStep",
                "position": 1,
                "name": catalog.t(locale, "landing.step1_title"),
                "text": catalog.t(locale, "landing.step1_body")
            },
            {
                "@type": "HowToStep",
                "position": 2,
                "name": catalog.t(locale, "landing.step2_title"),
                "text": catalog.t(locale, "landing.step2_body")
            },
            {
                "@type": "HowToStep",
                "position": 3,
                "name": catalog.t(locale, "landing.step3_title"),
                "text": catalog.t(locale, "landing.step3_body")
            }
        ]
    });

    let faq_pairs: Vec<(String, String)> = LANDING_FAQS
        .iter()
        .map(|item| {
            (
                catalog.t(locale, item.q_key).to_string(),
                catalog.t(locale, item.a_key).to_string(),
            )
        })
        .collect();

    let webpage = json!({
        "@type": "WebPage",
        "@id": format!("{canonical}#webpage"),
        "url": canonical,
        "name": title,
        "description": description,
        "inLanguage": locale.code(),
        "isPartOf": { "@id": format!("{base}/#website") },
        "about": { "@id": format!("{base}/#software") },
        "primaryImageOfPage": {
            "@type": "ImageObject",
            "url": og_img,
            "width": 1200,
            "height": 630
        }
    });

    safe_json_ld(&json!({
        "@context": "https://schema.org",
        "@graph": [
            organization_node(base),
            website_node(base),
            software,
            webpage,
            howto,
            faq_node(canonical, locale, &faq_pairs)
        ]
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn hub_json_ld(
    base_url: &str,
    locale: Locale,
    canonical: &str,
    title: &str,
    description: &str,
    home_label: &str,
    home_url: &str,
    hub_label: &str,
) -> String {
    let base = normalized_base_url(base_url);
    safe_json_ld(&json!({
        "@context": "https://schema.org",
        "@graph": [
            organization_node(base),
            website_node(base),
            {
                "@type": "CollectionPage",
                "@id": format!("{canonical}#webpage"),
                "url": canonical,
                "name": title,
                "description": description,
                "inLanguage": locale.code(),
                "isPartOf": { "@id": format!("{base}/#website") }
            },
            breadcrumb_node(canonical, vec![(home_label, home_url), (hub_label, canonical)])
        ]
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn segment_json_ld(
    base_url: &str,
    locale: Locale,
    canonical: &str,
    seg: &SegmentContent,
    home_label: &str,
    home_url: &str,
    hub_label: &str,
    hub_url: &str,
) -> String {
    let base = normalized_base_url(base_url);
    let faq_pairs: Vec<(String, String)> = seg
        .faqs
        .iter()
        .map(|(q, a)| ((*q).to_string(), (*a).to_string()))
        .collect();
    safe_json_ld(&json!({
        "@context": "https://schema.org",
        "@graph": [
            organization_node(base),
            website_node(base),
            {
                "@type": "WebPage",
                "@id": format!("{canonical}#webpage"),
                "url": canonical,
                "name": seg.meta_title,
                "description": seg.meta_description,
                "inLanguage": locale.code(),
                "isPartOf": { "@id": format!("{base}/#website") },
                "about": { "@id": format!("{base}/#software") }
            },
            breadcrumb_node(
                canonical,
                vec![(home_label, home_url), (hub_label, hub_url), (seg.name, canonical)],
            ),
            faq_node(canonical, locale, &faq_pairs)
        ]
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn tool_json_ld(
    base_url: &str,
    locale: Locale,
    canonical: &str,
    tool: &ToolContent,
    home_label: &str,
    home_url: &str,
    hub_label: &str,
    hub_url: &str,
) -> String {
    let base = normalized_base_url(base_url);
    let faq_pairs: Vec<(String, String)> = tool
        .faqs
        .iter()
        .map(|(q, a)| ((*q).to_string(), (*a).to_string()))
        .collect();
    safe_json_ld(&json!({
        "@context": "https://schema.org",
        "@graph": [
            organization_node(base),
            website_node(base),
            {
                "@type": "WebApplication",
                "@id": format!("{canonical}#webapp"),
                "name": tool.name,
                "description": tool.meta_description,
                "url": canonical,
                "applicationCategory": "UtilitiesApplication",
                "operatingSystem": "Web",
                "inLanguage": locale.code(),
                "isAccessibleForFree": true,
                "offers": {
                    "@type": "Offer",
                    "price": "0",
                    "priceCurrency": "BRL",
                    "availability": "https://schema.org/InStock"
                },
                "publisher": { "@id": format!("{base}/#organization") }
            },
            breadcrumb_node(
                canonical,
                vec![(home_label, home_url), (hub_label, hub_url), (tool.name, canonical)],
            ),
            faq_node(canonical, locale, &faq_pairs)
        ]
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn guide_json_ld(
    base_url: &str,
    locale: Locale,
    canonical: &str,
    guide: &GuideContent,
    home_label: &str,
    home_url: &str,
    hub_label: &str,
    hub_url: &str,
) -> String {
    let base = normalized_base_url(base_url);
    let og_img = og_image_url(base, locale);
    let faq_pairs: Vec<(String, String)> = guide
        .faqs
        .iter()
        .map(|(q, a)| ((*q).to_string(), (*a).to_string()))
        .collect();
    safe_json_ld(&json!({
        "@context": "https://schema.org",
        "@graph": [
            organization_node(base),
            website_node(base),
            {
                "@type": "Article",
                "@id": format!("{canonical}#article"),
                "headline": guide.title,
                "description": guide.meta_description,
                "inLanguage": locale.code(),
                "datePublished": guide.published_date,
                "dateModified": guide.published_date,
                "image": og_img,
                "mainEntityOfPage": canonical,
                "author": { "@id": format!("{base}/#organization") },
                "publisher": { "@id": format!("{base}/#organization") }
            },
            breadcrumb_node(
                canonical,
                vec![(home_label, home_url), (hub_label, hub_url), (guide.title, canonical)],
            ),
            faq_node(canonical, locale, &faq_pairs)
        ]
    }))
}

pub fn public_menu_json_ld(
    base_url: &str,
    snapshot: &MenuSnapshot,
    cover_url_rel: Option<&str>,
    logo_url_rel: Option<&str>,
) -> String {
    let base = normalized_base_url(base_url);
    let est = &snapshot.establishment;
    let menu_url = format!("{base}/c/{}", est.slug);

    let sections: Vec<Value> = snapshot
        .categories
        .iter()
        .filter(|sc| !sc.products.is_empty())
        .map(|sc| {
            let items: Vec<Value> = sc
                .products
                .iter()
                .map(|sp| {
                    let p = &sp.product;
                    let availability = if p.is_available {
                        "https://schema.org/InStock"
                    } else {
                        "https://schema.org/OutOfStock"
                    };
                    let mut item = json!({
                        "@type": "MenuItem",
                        "name": p.name,
                        "offers": {
                            "@type": "Offer",
                            "price": format_decimal_price(p.price_centavos),
                            "priceCurrency": "BRL",
                            "availability": availability
                        }
                    });
                    if !p.description.trim().is_empty() {
                        item["description"] = json!(p.description);
                    }
                    item
                })
                .collect();
            let mut section = json!({
                "@type": "MenuSection",
                "name": sc.category.name,
                "hasMenuItem": items
            });
            if !sc.category.description.trim().is_empty() {
                section["description"] = json!(sc.category.description);
            }
            section
        })
        .collect();

    let mut establishment = json!({
        "@type": "FoodEstablishment",
        "@id": format!("{menu_url}#establishment"),
        "name": est.name,
        "url": menu_url,
        "hasMenu": { "@id": format!("{menu_url}#menu") }
    });
    if !est.description.trim().is_empty() {
        establishment["description"] = json!(est.description);
    }
    if let Some(cover) = cover_url_rel {
        establishment["image"] = json!(absolute_url(base, cover));
    }
    if let Some(logo) = logo_url_rel {
        establishment["logo"] = json!(absolute_url(base, logo));
    }
    if let Some(maps) = &est.google_maps_url
        && !maps.trim().is_empty()
    {
        establishment["sameAs"] = json!([maps]);
    }

    let menu = json!({
        "@type": "Menu",
        "@id": format!("{menu_url}#menu"),
        "name": est.name,
        "url": menu_url,
        "inLanguage": "pt-BR",
        "hasMenuSection": sections
    });

    safe_json_ld(&json!({
        "@context": "https://schema.org",
        "@graph": [establishment, menu]
    }))
}
