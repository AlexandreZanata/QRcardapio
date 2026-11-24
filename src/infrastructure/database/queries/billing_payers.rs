use sqlx::{Row, SqliteExecutor};

use crate::domain::payer::{PayerProfile, PersonKind};
use crate::error::AppResult;

/// Payer identification of one establishment (personal data; tenant-scoped).
pub async fn find<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
) -> AppResult<Option<PayerProfile>> {
    let row = sqlx::query(
        "SELECT person_kind, document, legal_name, email, postal_code, street, number, complement,
                district, city, state
         FROM billing_payer_profiles WHERE establishment_id = ?",
    )
    .bind(establishment_id)
    .fetch_optional(executor)
    .await?;
    Ok(row.and_then(|r| {
        Some(PayerProfile {
            kind: PersonKind::parse(r.get("person_kind"))?,
            document: r.get("document"),
            legal_name: r.get("legal_name"),
            email: r.get("email"),
            postal_code: r.get("postal_code"),
            street: r.get("street"),
            number: r.get("number"),
            complement: r.get("complement"),
            district: r.get("district"),
            city: r.get("city"),
            state: r.get("state"),
        })
    }))
}

pub async fn upsert<'e>(
    executor: impl SqliteExecutor<'e>,
    establishment_id: &str,
    payer: &PayerProfile,
    account_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO billing_payer_profiles (
            establishment_id, person_kind, document, legal_name, email, postal_code, street, number,
            complement, district, city, state, updated_by_account_id, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(establishment_id) DO UPDATE SET
            person_kind = excluded.person_kind, document = excluded.document,
            legal_name = excluded.legal_name, email = excluded.email,
            postal_code = excluded.postal_code, street = excluded.street, number = excluded.number,
            complement = excluded.complement, district = excluded.district, city = excluded.city,
            state = excluded.state, updated_by_account_id = excluded.updated_by_account_id,
            updated_at = excluded.updated_at",
    )
    .bind(establishment_id)
    .bind(payer.kind.as_str())
    .bind(&payer.document)
    .bind(&payer.legal_name)
    .bind(&payer.email)
    .bind(&payer.postal_code)
    .bind(&payer.street)
    .bind(&payer.number)
    .bind(&payer.complement)
    .bind(&payer.district)
    .bind(&payer.city)
    .bind(&payer.state)
    .bind(account_id)
    .bind(now)
    .execute(executor)
    .await?;
    Ok(())
}
