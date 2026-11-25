use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::domain::commerce::order::{Order, OrderItemSnapshot};
use crate::error::{AppError, AppResult};

pub(crate) async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    order: &Order,
    items: &[OrderItemSnapshot<'_>],
) -> AppResult<()> {
    for (position, item) in items.iter().enumerate() {
        let position =
            i64::try_from(position).map_err(|_| AppError::Validation("commerce.invalid_order"))?;
        let item_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO order_items (
                id, establishment_id, order_id, position, product_id, name, name_en, name_es,
                variation_id, variation_name, variation_name_en, variation_name_es,
                base_price_centavos, unit_price_centavos, quantity, line_total_centavos
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item_id)
        .bind(&order.establishment_id)
        .bind(&order.id)
        .bind(position)
        .bind(&item.product.id)
        .bind(&item.product.name)
        .bind(&item.product.name_en)
        .bind(&item.product.name_es)
        .bind(item.variation.map(|variation| &variation.id))
        .bind(item.variation.map(|variation| &variation.name))
        .bind(item.variation.map(|variation| &variation.name_en))
        .bind(item.variation.map(|variation| &variation.name_es))
        .bind(item.base_price_centavos)
        .bind(item.price.unit_price_centavos)
        .bind(item.quantity)
        .bind(item.price.line_total_centavos)
        .execute(&mut **tx)
        .await?;

        for addon in &item.addons {
            sqlx::query(
                "INSERT INTO order_item_options (
                    establishment_id, order_item_id, option_id, group_id,
                    group_name, group_name_en, group_name_es,
                    name, name_en, name_es, unit_price_centavos, quantity
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&order.establishment_id)
            .bind(&item_id)
            .bind(&addon.option.id)
            .bind(&addon.group.id)
            .bind(&addon.group.name)
            .bind(&addon.group.name_en)
            .bind(&addon.group.name_es)
            .bind(&addon.option.name)
            .bind(&addon.option.name_en)
            .bind(&addon.option.name_es)
            .bind(addon.option.price_centavos)
            .bind(addon.quantity)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
