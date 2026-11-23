mod heuristics;
mod price;

use std::collections::HashSet;

use super::establishment::sanitize_optional_text;
use super::ocr::{OcrConfidence, ParsedMenuDraft, ParsedMenuItem};

use heuristics::{
    clean_line, is_likely_category_heading, is_noise_line, looks_like_description,
    strip_decorative_markers,
};
use price::{extract_items_from_priced_line, parse_standalone_price, strip_trailing_leaders};

/// Parses raw OCR or PDF text into structured menu categories and items with confidence ratings.
pub fn parse_menu_text(raw_text: &str, default_category: &str) -> ParsedMenuDraft {
    let lines: Vec<String> = raw_text
        .lines()
        .map(clean_line)
        .filter(|l| !l.is_empty())
        .collect();

    let mut current_category = default_category.trim().to_string();
    if current_category.is_empty() {
        current_category = "Cardápio Geral".to_string();
    }

    let mut items: Vec<ParsedMenuItem> = Vec::new();
    let mut pending_unpriced: Option<String> = None;
    let mut idx = 0usize;

    while idx < lines.len() {
        let line = &lines[idx];
        if is_noise_line(line) {
            idx += 1;
            continue;
        }

        // Case 1: Standalone price line attaches to a preceding unpriced item name.
        if let Some((centavos, corrected)) = parse_standalone_price(line) {
            if let Some(prev_name) = pending_unpriced.take() {
                items.push(make_item(
                    &current_category,
                    &prev_name,
                    "",
                    Some(centavos),
                    if corrected {
                        OcrConfidence::Medium
                    } else {
                        OcrConfidence::High
                    },
                ));
            } else if let Some(last) = items.last_mut()
                && last.price_centavos.is_none()
            {
                last.price_centavos = Some(centavos);
                last.confidence = OcrConfidence::Medium;
            }
            idx += 1;
            continue;
        }

        // Case 2: Line containing one or more trailing/inline prices (e.g. "X-Bacon ..... R$ 28,90").
        let extracted = extract_items_from_priced_line(line);
        if !extracted.is_empty() {
            pending_unpriced = None;
            for (name, centavos, conf) in extracted {
                items.push(make_item(
                    &current_category,
                    &name,
                    "",
                    Some(centavos),
                    conf,
                ));
            }
            idx += 1;
            continue;
        }

        // Case 3: Category heading detection.
        let next_line = lines.get(idx + 1).map(String::as_str);
        if is_likely_category_heading(
            line,
            next_line,
            items.is_empty() && pending_unpriced.is_none(),
        ) {
            flush_pending(&mut pending_unpriced, &current_category, &mut items);
            current_category = strip_decorative_markers(line);
            idx += 1;
            continue;
        }

        // Case 4: Description line immediately following an item on the previous line.
        let next_is_standalone_price = next_line.and_then(parse_standalone_price).is_some();

        if !next_is_standalone_price
            && pending_unpriced.is_none()
            && let Some(last) = items.last_mut()
            && last.description.len() < 240
            && looks_like_description(line)
        {
            if !last.description.is_empty() {
                last.description.push(' ');
            }
            last.description.push_str(line);
            last.description = sanitize_optional_text(&last.description, 320);
            idx += 1;
            continue;
        }

        // Case 5: Possible item name whose price appears on the next line (or missing price).
        if let Some(prev) = pending_unpriced.take()
            && prev.chars().count() >= 3
            && prev.chars().count() <= 80
        {
            items.push(make_item(
                &current_category,
                &prev,
                "",
                None,
                OcrConfidence::Low,
            ));
        }
        pending_unpriced = Some(line.clone());
        idx += 1;
    }

    flush_pending(&mut pending_unpriced, &current_category, &mut items);

    let mut categories = Vec::new();
    let mut seen = HashSet::new();
    for item in &items {
        let key = item.category_name.to_lowercase();
        if seen.insert(key) {
            categories.push(item.category_name.clone());
        }
    }

    let mut warning_keys = Vec::new();
    if items.is_empty() {
        warning_keys.push("ocr.warn_no_items".to_string());
    }
    if items.iter().any(|i| i.price_centavos.is_none()) {
        warning_keys.push("ocr.warn_missing_prices".to_string());
    }

    ParsedMenuDraft {
        categories,
        items,
        warning_keys,
    }
}

fn flush_pending(pending: &mut Option<String>, category: &str, items: &mut Vec<ParsedMenuItem>) {
    if let Some(name) = pending.take()
        && name.chars().count() >= 3
        && name.chars().count() <= 80
        && !is_noise_line(&name)
    {
        items.push(make_item(category, &name, "", None, OcrConfidence::Low));
    }
}

fn make_item(
    category: &str,
    name: &str,
    desc: &str,
    price_centavos: Option<i64>,
    confidence: OcrConfidence,
) -> ParsedMenuItem {
    let clean_name = sanitize_optional_text(&strip_trailing_leaders(name), 100);
    let clean_cat = sanitize_optional_text(category, 80);
    let clean_desc = sanitize_optional_text(desc, 320);
    let final_conf = if clean_name.chars().count() < 3 || price_centavos.is_none() {
        OcrConfidence::Low
    } else {
        confidence
    };

    ParsedMenuItem {
        category_name: if clean_cat.is_empty() {
            "Cardápio Geral".to_string()
        } else {
            clean_cat
        },
        name: clean_name,
        description: clean_desc,
        price_centavos,
        confidence: final_conf,
        is_available: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brazilian_menu_with_categories_descriptions_and_leaders() {
        let sample = r#"
CARDÁPIO
@cafedapraca
(11) 99999-1234

CAFÉS ESPECIAIS
Espresso Duplo ........................ R$ 9,50
Grãos da Serra da Mantiqueira, torra média
Cappuccino Cremoso .................... R$ 14,90
Leite vaporizado com cacau 70%

LANCHES
X-Salada Artesanal                     24,90
Pão brioche, blend 180g, queijo prato e salada
Pão de Queijo
R$ 8,00

AÇAÍ
Açaí 300ml R$ 15,00 | 500ml R$ 22,00
"#;
        let draft = parse_menu_text(sample, "Cardápio Geral");
        assert_eq!(draft.categories, vec!["CAFÉS ESPECIAIS", "LANCHES", "AÇAÍ"]);
        assert_eq!(draft.items.len(), 6);

        assert_eq!(draft.items[0].name, "Espresso Duplo");
        assert_eq!(draft.items[0].price_centavos, Some(950));
        assert!(draft.items[0].description.contains("Mantiqueira"));

        assert_eq!(draft.items[2].name, "X-Salada Artesanal");
        assert_eq!(draft.items[2].price_centavos, Some(2490));
        assert!(draft.items[2].description.contains("brioche"));

        assert_eq!(draft.items[3].name, "Pão de Queijo");
        assert_eq!(draft.items[3].price_centavos, Some(800));

        assert_eq!(draft.items[4].name, "Açaí 300ml");
        assert_eq!(draft.items[4].price_centavos, Some(1500));
        assert_eq!(draft.items[5].name, "Açaí 500ml");
        assert_eq!(draft.items[5].price_centavos, Some(2200));
    }

    #[test]
    fn corrects_ocr_digit_artifacts_and_ignores_non_price_numbers() {
        let sample = "BEBIDAS\nSuco Natural 500ml\nRS 1O,5O\nÁgua Mineral 500ml .... R$ 5,00";
        let draft = parse_menu_text(sample, "Geral");
        assert_eq!(draft.items.len(), 2);
        assert_eq!(draft.items[0].name, "Suco Natural 500ml");
        assert_eq!(draft.items[0].price_centavos, Some(1050));
        assert_eq!(draft.items[0].confidence, OcrConfidence::Medium);
        assert_eq!(draft.items[1].price_centavos, Some(500));
    }
}
