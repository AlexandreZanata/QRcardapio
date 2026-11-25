//! Payer identification required by bank-issued charges (Inter boleto/Pix). Personal data:
//! collected only from owners who choose Pix/boleto, tenant-scoped, never logged or sent to
//! events (LGPD).

use crate::domain::account::Email;
use crate::error::{AppError, AppResult};

const UFS: [&str; 27] = [
    "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA", "PB", "PE",
    "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonKind {
    Individual,
    Company,
}

impl PersonKind {
    /// Value expected by Inter (`tipoPessoa`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "FISICA",
            Self::Company => "JURIDICA",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "FISICA" => Some(Self::Individual),
            "JURIDICA" => Some(Self::Company),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayerProfile {
    pub kind: PersonKind,
    /// CPF (11) or CNPJ (14) digits only.
    pub document: String,
    pub legal_name: String,
    pub email: String,
    pub postal_code: String,
    pub street: String,
    pub number: String,
    pub complement: String,
    pub district: String,
    pub city: String,
    pub state: String,
}

pub struct PayerInput<'a> {
    pub document: &'a str,
    pub legal_name: &'a str,
    pub email: &'a str,
    pub postal_code: &'a str,
    pub street: &'a str,
    pub number: &'a str,
    pub complement: &'a str,
    pub district: &'a str,
    pub city: &'a str,
    pub state: &'a str,
}

fn digits(raw: &str) -> String {
    raw.chars().filter(char::is_ascii_digit).collect()
}

fn text(raw: &str, min: usize, max: usize) -> AppResult<String> {
    let value: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let len = value.chars().count();
    if (min..=max).contains(&len) {
        Ok(value)
    } else {
        Err(AppError::Validation("error.payer_invalid"))
    }
}

fn check_digit(digits: &[u32], weights: &[u32]) -> u32 {
    let sum: u32 = digits.iter().zip(weights).map(|(d, w)| d * w).sum();
    match sum % 11 {
        0 | 1 => 0,
        rest => 11 - rest,
    }
}

/// Validates CPF check digits (rejects repeated-digit sequences).
pub fn valid_cpf(cpf: &str) -> bool {
    let d: Vec<u32> = cpf.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() != 11 || cpf.len() != 11 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    let first = check_digit(&d[..9], &[10, 9, 8, 7, 6, 5, 4, 3, 2]);
    let second = check_digit(&d[..10], &[11, 10, 9, 8, 7, 6, 5, 4, 3, 2]);
    d[9] == first && d[10] == second
}

/// Validates CNPJ check digits (numeric CNPJ; rejects repeated-digit sequences).
pub fn valid_cnpj(cnpj: &str) -> bool {
    let d: Vec<u32> = cnpj.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() != 14 || cnpj.len() != 14 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    let first = check_digit(&d[..12], &[5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]);
    let second = check_digit(&d[..13], &[6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]);
    d[12] == first && d[13] == second
}

impl PayerProfile {
    pub fn parse(input: PayerInput<'_>) -> AppResult<Self> {
        let document = digits(input.document);
        let kind = match document.len() {
            11 if valid_cpf(&document) => PersonKind::Individual,
            14 if valid_cnpj(&document) => PersonKind::Company,
            _ => return Err(AppError::Validation("error.payer_document_invalid")),
        };
        let postal_code = digits(input.postal_code);
        if postal_code.len() != 8 {
            return Err(AppError::Validation("error.payer_postal_code_invalid"));
        }
        let state = input.state.trim().to_ascii_uppercase();
        if !UFS.contains(&state.as_str()) {
            return Err(AppError::Validation("error.payer_state_invalid"));
        }
        let email = Email::parse(input.email)
            .map_err(|_| AppError::Validation("error.invalid_email"))?
            .as_str()
            .to_owned();
        Ok(Self {
            kind,
            document,
            legal_name: text(input.legal_name, 3, 100)?,
            email,
            postal_code,
            street: text(input.street, 2, 90)?,
            number: text(input.number, 1, 10)?,
            complement: text(input.complement, 0, 30)?,
            district: text(input.district, 2, 60)?,
            city: text(input.city, 2, 60)?,
            state,
        })
    }

    /// Masked document for display (`**.***.***/0001-**`).
    pub fn masked_document(&self) -> String {
        let keep = if self.document.len() == 14 { 4 } else { 3 };
        let visible_from = self.document.len() - keep - 2;
        self.document
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if (visible_from..visible_from + keep).contains(&i) {
                    c
                } else {
                    '*'
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(document: &str) -> PayerInput<'_> {
        PayerInput {
            document,
            legal_name: "  Bistrô   da Praça Ltda ",
            email: "Financeiro@Bistro.test",
            postal_code: "01310-100",
            street: "Avenida Paulista",
            number: "1000",
            complement: "",
            district: "Bela Vista",
            city: "São Paulo",
            state: "sp",
        }
    }

    #[test]
    fn validates_documents_with_check_digits() {
        assert!(valid_cpf("52998224725"));
        assert!(!valid_cpf("52998224724"));
        assert!(!valid_cpf("11111111111"));
        assert!(valid_cnpj("11222333000181"));
        assert!(!valid_cnpj("11222333000180"));
        assert!(!valid_cnpj("00000000000000"));
    }

    #[test]
    fn normalizes_a_company_payer() {
        let payer = PayerProfile::parse(input("11.222.333/0001-81")).unwrap();
        assert_eq!(payer.kind, PersonKind::Company);
        assert_eq!(payer.document, "11222333000181");
        assert_eq!(payer.legal_name, "Bistrô da Praça Ltda");
        assert_eq!(payer.email, "financeiro@bistro.test");
        assert_eq!(payer.postal_code, "01310100");
        assert_eq!(payer.state, "SP");
        assert_eq!(payer.masked_document(), "********0001**");
        let person = PayerProfile::parse(input("529.982.247-25")).unwrap();
        assert_eq!(person.kind, PersonKind::Individual);
        assert_eq!(person.masked_document(), "******247**");
        assert!(PayerProfile::parse(input("123")).is_err());
        let mut bad_state = input("52998224725");
        bad_state.state = "XX";
        assert!(PayerProfile::parse(bad_state).is_err());
    }
}
