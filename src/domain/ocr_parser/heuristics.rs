use super::price::extract_items_from_priced_line;

const KNOWN_CATEGORIES: &[&str] = &[
    "entradas",
    "petiscos",
    "porcoes",
    "porções",
    "aperitivos",
    "saladas",
    "sopas",
    "caldos",
    "lanches",
    "sanduiches",
    "sanduíches",
    "hamburgueres",
    "hambúrgueres",
    "burgers",
    "artesanais",
    "hot dogs",
    "cachorro quente",
    "pizzas",
    "pizzas tradicionais",
    "pizzas especiais",
    "pizzas doces",
    "esfihas",
    "pasteis",
    "pastéis",
    "salgados",
    "paes",
    "pães",
    "massas",
    "risotos",
    "carnes",
    "grelhados",
    "peixes",
    "frutos do mar",
    "pratos principais",
    "pratos executivos",
    "executivos",
    "refeicoes",
    "refeições",
    "combos",
    "acompanhamentos",
    "adicionais",
    "sobremesas",
    "doces",
    "bolos",
    "tortas",
    "sorvetes",
    "gelatos",
    "acai",
    "açaí",
    "milkshakes",
    "bebidas",
    "bebidas geladas",
    "bebidas quentes",
    "sucos",
    "sucos naturais",
    "refrigerantes",
    "cervejas",
    "chopp",
    "vinhos",
    "drinks",
    "coqueteis",
    "coquetéis",
    "cafes",
    "cafés",
    "cafes especiais",
    "cafés especiais",
    "chas",
    "chás",
    "cafe da manha",
    "café da manhã",
    "starters",
    "appetizers",
    "mains",
    "main courses",
    "sides",
    "desserts",
    "beverages",
    "coffee",
    "specialty coffee",
    "postres",
    "platos principales",
    "bebidas frias",
    "bebidas frías",
];

const NOISE_PREFIXES: &[&str] = &[
    "http://",
    "https://",
    "www.",
    "instagram",
    "facebook",
    "whatsapp",
    "wifi",
    "wi-fi",
    "senha",
    "cnpj",
    "telefone",
    "fone:",
    "tel:",
    "delivery:",
    "endereco",
    "endereço",
    "taxa de servico",
    "taxa de serviço",
    "aceitamos",
    "página ",
    "pagina ",
    "page ",
];

pub(super) fn clean_line(raw: &str) -> String {
    let trimmed = raw.trim().replace('\t', "  ");
    let mut out = String::with_capacity(trimmed.len());
    let mut spaces = 0usize;
    for ch in trimmed.chars() {
        if ch.is_control() {
            continue;
        }
        if ch == ' ' {
            spaces += 1;
            if spaces <= 2 {
                out.push(' ');
            }
        } else {
            spaces = 0;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

pub(super) fn is_noise_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.len() < 2 {
        return true;
    }
    if lower.starts_with('@') || NOISE_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    if !line.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    if matches!(
        lower.as_str(),
        "cardapio" | "cardápio" | "menu" | "menú" | "cardapio digital" | "cardápio digital"
    ) {
        return true;
    }
    let digits = line.chars().filter(|c| c.is_ascii_digit()).count();
    let letters = line.chars().filter(|c| c.is_alphabetic()).count();
    digits >= 8 && letters <= 4 && !lower.contains("r$")
}

pub(super) fn strip_decorative_markers(line: &str) -> String {
    line.trim_matches(|c: char| {
        matches!(
            c,
            '-' | '=' | '*' | '•' | '·' | '—' | '–' | '|' | ':' | '#' | ' '
        )
    })
    .trim()
    .to_string()
}

pub(super) fn is_likely_category_heading(
    line: &str,
    next_line: Option<&str>,
    at_start: bool,
) -> bool {
    let stripped = strip_decorative_markers(line);
    if stripped.is_empty() || stripped.chars().count() > 45 {
        return false;
    }
    let lower = stripped.to_lowercase();
    if KNOWN_CATEGORIES.contains(&lower.as_str()) {
        return true;
    }

    let word_count = stripped.split_whitespace().count();
    let has_digits = stripped.chars().any(|c| c.is_ascii_digit());
    let ends_sentence = stripped.ends_with('.');
    if word_count == 0 || word_count > 5 || has_digits || ends_sentence {
        return false;
    }

    let alpha_chars: Vec<char> = stripped.chars().filter(|c| c.is_alphabetic()).collect();
    if alpha_chars.len() < 3 {
        return false;
    }

    let all_upper = alpha_chars.iter().all(|c| c.is_uppercase());
    let decorated = line.starts_with("---")
        || line.starts_with("===")
        || line.starts_with("***")
        || line.starts_with('•')
        || line.ends_with(':');
    let next_has_price = next_line
        .map(|n| !extract_items_from_priced_line(n).is_empty())
        .unwrap_or(false);

    if all_upper && (next_has_price || at_start || decorated) {
        return true;
    }
    decorated && word_count <= 4
}

pub(super) fn looks_like_description(line: &str) -> bool {
    let trimmed = line.trim();
    let word_count = trimmed.split_whitespace().count();
    let has_comma = trimmed.contains(',');
    let has_lowercase = trimmed.chars().any(|c| c.is_lowercase());
    let starts_lower = trimmed.chars().next().is_some_and(|c| c.is_lowercase());

    if starts_lower || has_comma || word_count >= 5 {
        return has_lowercase && trimmed.chars().count() <= 240;
    }
    false
}
