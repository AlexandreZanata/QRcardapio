//! Inter Cobrança v3 payloads: request body, charge snapshot and callback parsing.

use serde_json::{Number, Value, json};

use super::{ChargeSnapshot, NewCharge};
use crate::domain::payments::centavos_to_decimal;
use crate::domain::product::parse_price_centavos;

/// Inter request codes are UUIDs.
pub fn valid_request_code(code: &str) -> bool {
    uuid::Uuid::parse_str(code).is_ok() && code.len() == 36
}

pub(super) fn charge_body(charge: &NewCharge<'_>) -> Value {
    let amount: Number = centavos_to_decimal(charge.amount_centavos)
        .parse()
        .expect("decimal amount is a valid JSON number");
    let payer = charge.payer;
    let mut pagador = json!({
        "cpfCnpj": payer.document,
        "tipoPessoa": payer.kind.as_str(),
        "nome": payer.legal_name,
        "endereco": payer.street,
        "numero": payer.number,
        "bairro": payer.district,
        "cidade": payer.city,
        "uf": payer.state,
        "cep": payer.postal_code,
    });
    if !payer.complement.is_empty() {
        pagador["complemento"] = json!(payer.complement);
    }
    if payer.email.len() <= 50 {
        pagador["email"] = json!(payer.email);
    }
    json!({
        "seuNumero": charge.our_number,
        "valorNominal": amount,
        "dataVencimento": charge.due_date.to_string(),
        "numDiasAgenda": charge.payable_days_after_due,
        "pagador": pagador,
        "formasRecebimento": ["BOLETO", "PIX"],
        "mensagem": { "linha1": charge.message.chars().take(78).collect::<String>() },
    })
}

fn amount_centavos(value: Option<&Value>) -> Option<i64> {
    let raw = match value? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    parse_price_centavos(&raw).ok()
}

pub(super) fn parse_snapshot(body: &Value) -> Option<ChargeSnapshot> {
    let charge = body.get("cobranca")?;
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
    let request_code = text(charge, "codigoSolicitacao").filter(|c| valid_request_code(c))?;
    Some(ChargeSnapshot {
        request_code,
        our_number: text(charge, "seuNumero"),
        status: text(charge, "situacao")
            .filter(|s| s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))?,
        due_date: text(charge, "dataVencimento").and_then(|d| d.parse().ok()),
        received_centavos: amount_centavos(charge.get("valorTotalRecebido")).filter(|c| *c > 0),
        status_date: text(charge, "dataSituacao"),
        pix_copy_paste: body.get("pix").and_then(|p| text(p, "pixCopiaECola")),
        digitable_line: body.get("boleto").and_then(|b| text(b, "linhaDigitavel")),
    })
}

/// Request codes announced by a Cobrança callback (a JSON array; objects are tolerated).
pub fn callback_request_codes(body: &Value) -> Vec<String> {
    let items: Vec<&Value> = match body {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![body],
        _ => Vec::new(),
    };
    let mut codes: Vec<String> = items
        .into_iter()
        .filter_map(|item| item.get("codigoSolicitacao").and_then(Value::as_str))
        .filter(|code| valid_request_code(code))
        .map(str::to_owned)
        .collect();
    codes.sort();
    codes.dedup();
    codes.truncate(100);
    codes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::payer::{PayerInput, PayerProfile};
    use chrono::NaiveDate;

    fn payer() -> PayerProfile {
        PayerProfile::parse(PayerInput {
            document: "11.222.333/0001-81",
            legal_name: "Bistro da Praca Ltda",
            email: "financeiro@bistro.test",
            postal_code: "01310-100",
            street: "Avenida Paulista",
            number: "1000",
            complement: "",
            district: "Bela Vista",
            city: "Sao Paulo",
            state: "SP",
        })
        .unwrap()
    }

    #[test]
    fn charge_body_uses_integer_amounts_and_required_payer_fields() {
        let payer = payer();
        let body = charge_body(&NewCharge {
            our_number: "Q123",
            amount_centavos: 17_043,
            due_date: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            payable_days_after_due: 5,
            payer: &payer,
            message: "QR Cardapio",
        });
        assert_eq!(body["valorNominal"].to_string(), "170.43");
        assert_eq!(body["seuNumero"], "Q123");
        assert_eq!(body["pagador"]["cpfCnpj"], "11222333000181");
        assert_eq!(body["pagador"]["tipoPessoa"], "JURIDICA");
        assert_eq!(body["pagador"]["cep"], "01310100");
        assert!(body["pagador"].get("complemento").is_none());
        assert_eq!(body["formasRecebimento"], json!(["BOLETO", "PIX"]));
    }

    #[test]
    fn parses_snapshots_and_callbacks_leniently() {
        let code = "0193a0c0-0000-7000-8000-00000000000a";
        let body = json!({
            "cobranca": {"codigoSolicitacao": code, "seuNumero": "Q1", "situacao": "RECEBIDO",
                         "dataVencimento": "2026-12-01", "valorTotalRecebido": "29.90"},
            "boleto": {"linhaDigitavel": "0001"}, "pix": {"pixCopiaECola": "000201"}
        });
        let snapshot = parse_snapshot(&body).unwrap();
        assert_eq!(snapshot.received_centavos, Some(2990));
        assert_eq!(snapshot.status, "RECEBIDO");
        assert_eq!(snapshot.pix_copy_paste.as_deref(), Some("000201"));
        let numeric = json!({"cobranca": {"codigoSolicitacao": code, "situacao": "RECEBIDO", "valorTotalRecebido": 29.9}});
        assert_eq!(
            parse_snapshot(&numeric).unwrap().received_centavos,
            Some(2990)
        );
        assert!(
            parse_snapshot(
                &json!({"cobranca": {"codigoSolicitacao": "x", "situacao": "RECEBIDO"}})
            )
            .is_none()
        );

        let callback = json!([{"codigoSolicitacao": code, "situacao": "RECEBIDO"},
                              {"codigoSolicitacao": code}, {"codigoSolicitacao": "../etc"}]);
        assert_eq!(callback_request_codes(&callback), [code]);
        assert_eq!(
            callback_request_codes(&json!({"codigoSolicitacao": code})),
            [code]
        );
    }
}
