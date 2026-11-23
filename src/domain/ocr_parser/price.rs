use crate::domain::ocr::OcrConfidence;
use crate::domain::product::parse_price_centavos;

pub(super) fn strip_trailing_leaders(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| {
            matches!(c, '.' | '·' | '-' | '_' | '—' | '–' | ':' | '|' | ' ')
        })
        .trim()
        .to_string()
}

/// Parses a line containing only a price (e.g. `R$ 19,90`, `RS 12,50`, `24,90`).
pub(super) fn parse_standalone_price(line: &str) -> Option<(i64, bool)> {
    let trimmed = line.trim();
    if trimmed.len() > 18 {
        return None;
    }
    parse_price_token(trimmed)
}

/// Extracts `(name, centavos, confidence)` from lines with trailing price or pipe multi-size variants.
pub(super) fn extract_items_from_priced_line(line: &str) -> Vec<(String, i64, OcrConfidence)> {
    if line.contains('|') {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() >= 2 {
            let mut results = Vec::new();
            let mut base_prefix = String::new();
            for (idx, part) in parts.iter().enumerate() {
                if let Some((name, price, conf)) = extract_single_trailing_price(part) {
                    if idx == 0 {
                        base_prefix = name.split_whitespace().next().unwrap_or("").to_string();
                        results.push((name, price, conf));
                    } else {
                        let full = if !base_prefix.is_empty()
                            && name.split_whitespace().count() == 1
                            && !name.to_lowercase().starts_with(&base_prefix.to_lowercase())
                        {
                            format!("{base_prefix} {name}")
                        } else {
                            name
                        };
                        results.push((full, price, conf));
                    }
                }
            }
            if results.len() >= 2 {
                return results;
            }
        }
    }

    extract_single_trailing_price(line)
        .map(|item| vec![item])
        .unwrap_or_default()
}

fn extract_single_trailing_price(line: &str) -> Option<(String, i64, OcrConfidence)> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }

    let try_lengths = if tokens.len() >= 3 {
        vec![2usize, 1usize]
    } else {
        vec![1usize]
    };

    for tail_len in try_lengths {
        let split_at = tokens.len() - tail_len;
        let tail = tokens[split_at..].join(" ");
        if let Some((centavos, corrected)) = parse_price_token(&tail) {
            let head = tokens[..split_at].join(" ");
            let clean_name = strip_trailing_leaders(&head);
            if clean_name.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                let conf = if corrected {
                    OcrConfidence::Medium
                } else {
                    OcrConfidence::High
                };
                return Some((clean_name, centavos, conf));
            }
        }
    }

    None
}

/// Parses a currency/price token into integer centavos, applying narrow OCR digit cleanup
/// only when a valid price pattern (`R$` prefix or decimal `,XX`/`.XX`) is present.
fn parse_price_token(raw: &str) -> Option<(i64, bool)> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    let upper = s.to_ascii_uppercase();
    let (has_currency_prefix, rest) = if let Some(r) = upper.strip_prefix("R$") {
        (true, r.trim())
    } else if let Some(r) = upper.strip_prefix("BRL") {
        (true, r.trim())
    } else if let Some(r) = upper.strip_prefix("RS ") {
        (true, r.trim())
    } else if let Some(r) = upper.strip_prefix("R5 ") {
        (true, r.trim())
    } else if let Some(r) = upper.strip_prefix('$') {
        (true, r.trim())
    } else {
        (false, s)
    };

    if rest.is_empty() {
        return None;
    }

    // Without a currency marker, require an explicit decimal separator (`,XX` or `.XX`)
    // so numbers like "500ml", "350g", or "2026" are never misread as prices.
    let has_decimal = rest.contains(',') || rest.contains('.');
    if !has_currency_prefix && !has_decimal {
        return None;
    }

    if let Ok(c) = parse_price_centavos(rest) {
        if c > 0 {
            return Some((c, false));
        }
        return None;
    }

    let mut corrected = String::with_capacity(rest.len());
    let mut changed = false;
    for ch in rest.chars() {
        match ch {
            '0'..='9' | ',' | '.' => corrected.push(ch),
            'O' | 'o' => {
                corrected.push('0');
                changed = true;
            }
            'l' | 'I' | '|' => {
                corrected.push('1');
                changed = true;
            }
            'S' => {
                corrected.push('5');
                changed = true;
            }
            _ => return None,
        }
    }

    if changed
        && let Ok(c) = parse_price_centavos(&corrected)
        && c > 0
    {
        return Some((c, true));
    }
    None
}
