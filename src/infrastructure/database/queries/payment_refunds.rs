use sqlx::{Sqlite, Transaction};

use crate::domain::commerce::order_payment::{OrderPayment, PaymentRefund};
use crate::error::AppResult;

pub(crate) async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    payment: &OrderPayment,
    refund: &PaymentRefund,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO payment_refunds (id, establishment_id, payment_id, amount_centavos, reason_code, recorded_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&refund.id)
    .bind(&payment.establishment_id)
    .bind(&payment.id)
    .bind(refund.amount_centavos)
    .bind(refund.reason.as_str())
    .bind(refund.recorded_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
