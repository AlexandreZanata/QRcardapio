//! Checked integer pricing for order lines and totals across sales channels.

use super::discount::Discount;

pub const MAX_ORDER_TOTAL_CENTAVOS: i64 = 1_000_000;
pub const MAX_LINE_QUANTITY: i64 = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PricingError {
    #[error("negative monetary amount")]
    NegativeAmount,
    #[error("quantity is outside its allowed range")]
    InvalidQuantity,
    #[error("order must contain at least one line")]
    EmptyOrder,
    #[error("discount is outside its allowed range")]
    InvalidDiscount,
    #[error("integer monetary calculation overflowed")]
    Overflow,
    #[error("order total exceeds the maximum")]
    OrderLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddonPrice {
    pub price_centavos: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct LinePrice {
    pub unit_price_centavos: i64,
    pub line_total_centavos: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderTotal {
    pub items_subtotal_centavos: i64,
    pub discount_centavos: i64,
    pub delivery_fee_centavos: i64,
    pub total_centavos: i64,
}

pub fn price_line(
    base_or_variation_centavos: i64,
    quantity: i64,
    addons: &[AddonPrice],
) -> Result<LinePrice, PricingError> {
    if base_or_variation_centavos < 0 {
        return Err(PricingError::NegativeAmount);
    }
    if !(1..=MAX_LINE_QUANTITY).contains(&quantity) {
        return Err(PricingError::InvalidQuantity);
    }
    let unit_price_centavos =
        addons
            .iter()
            .try_fold(base_or_variation_centavos, |unit, addon| {
                if addon.price_centavos < 0 {
                    return Err(PricingError::NegativeAmount);
                }
                if addon.quantity < 1 {
                    return Err(PricingError::InvalidQuantity);
                }
                let extra = addon
                    .price_centavos
                    .checked_mul(addon.quantity)
                    .ok_or(PricingError::Overflow)?;
                unit.checked_add(extra).ok_or(PricingError::Overflow)
            })?;
    let line_total_centavos = unit_price_centavos
        .checked_mul(quantity)
        .ok_or(PricingError::Overflow)?;
    Ok(LinePrice {
        unit_price_centavos,
        line_total_centavos,
    })
}

pub fn price_order(
    lines: &[LinePrice],
    discount: Option<Discount>,
    delivery_fee_centavos: i64,
) -> Result<OrderTotal, PricingError> {
    if lines.is_empty() {
        return Err(PricingError::EmptyOrder);
    }
    if delivery_fee_centavos < 0 {
        return Err(PricingError::NegativeAmount);
    }
    let items_subtotal_centavos = lines.iter().try_fold(0_i64, |subtotal, line| {
        if line.line_total_centavos < 0 {
            return Err(PricingError::NegativeAmount);
        }
        subtotal
            .checked_add(line.line_total_centavos)
            .ok_or(PricingError::Overflow)
    })?;
    let discount_centavos =
        discount.map_or(Ok(0), |discount| discount.amount(items_subtotal_centavos))?;
    let total_centavos = items_subtotal_centavos
        .checked_sub(discount_centavos)
        .and_then(|discounted| discounted.checked_add(delivery_fee_centavos))
        .ok_or(PricingError::Overflow)?;
    if total_centavos > MAX_ORDER_TOTAL_CENTAVOS {
        return Err(PricingError::OrderLimit);
    }
    Ok(OrderTotal {
        items_subtotal_centavos,
        discount_centavos,
        delivery_fee_centavos,
        total_centavos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity_amount_cap_and_overflow_boundaries() {
        assert_eq!(price_line(-1, 1, &[]), Err(PricingError::NegativeAmount));
        assert_eq!(price_line(1, 0, &[]), Err(PricingError::InvalidQuantity));
        assert_eq!(price_line(1, 100, &[]), Err(PricingError::InvalidQuantity));
        assert_eq!(price_line(i64::MAX, 2, &[]), Err(PricingError::Overflow));
        assert_eq!(
            price_line(
                i64::MAX,
                1,
                &[AddonPrice {
                    price_centavos: 1,
                    quantity: 1
                }]
            ),
            Err(PricingError::Overflow)
        );
        assert_eq!(
            price_line(
                0,
                1,
                &[AddonPrice {
                    price_centavos: i64::MAX,
                    quantity: 2
                }]
            ),
            Err(PricingError::Overflow)
        );
        let max = price_line(i64::MAX, 1, &[]).unwrap();
        assert_eq!(
            price_order(&[max, max], None, 0),
            Err(PricingError::Overflow)
        );
        assert_eq!(price_order(&[max], None, 1), Err(PricingError::Overflow));
        assert_eq!(price_order(&[], None, 0), Err(PricingError::EmptyOrder));
        let capped = price_line(MAX_ORDER_TOTAL_CENTAVOS, 1, &[]).unwrap();
        assert_eq!(
            price_order(&[capped], None, -1),
            Err(PricingError::NegativeAmount)
        );
        assert_eq!(
            price_order(&[capped], None, 1),
            Err(PricingError::OrderLimit)
        );
        assert_eq!(
            price_order(&[capped], None, 0).unwrap().total_centavos,
            MAX_ORDER_TOTAL_CENTAVOS
        );
        assert_eq!(
            price_order(&[max], Some(Discount::BasisPoints(10_000)), 1)
                .unwrap()
                .total_centavos,
            1
        );
    }

    #[test]
    fn generated_orders_reconcile_to_an_independent_integer_oracle() {
        // Exercise reproducible inputs across quantities, prices, rounding and over-cap quotes.
        for sample in 0_i64..20_000 {
            let base = sample * 7919 % 70_001;
            let quantity = sample * 37 % 99 + 1;
            let extra = sample * 4513 % 701;
            let extra_quantity = sample * 13 % 9 + 1;
            let other = sample * 2129 % 34_001;
            let fee = sample * 47 % 2201;
            let bps = sample * 6173 % 10_001;
            let lines = [
                price_line(
                    base,
                    quantity,
                    &[AddonPrice {
                        price_centavos: extra,
                        quantity: extra_quantity,
                    }],
                )
                .unwrap(),
                price_line(other, 1, &[]).unwrap(),
            ];
            let expected_unit = i128::from(base) + i128::from(extra) * i128::from(extra_quantity);
            let expected_line = expected_unit * i128::from(quantity);
            assert_eq!(i128::from(lines[0].unit_price_centavos), expected_unit);
            assert_eq!(i128::from(lines[0].line_total_centavos), expected_line);
            let subtotal = expected_line + i128::from(other);
            let fraction = subtotal * i128::from(bps);
            let percentage = fraction / 10_000 + i128::from(fraction % 10_000 >= 5_000);
            let fixed = i128::from(sample * 2003 % 6_000_001);
            for (discount, amount) in [
                (Discount::BasisPoints(bps), percentage),
                (Discount::FixedCentavos(fixed as i64), subtotal.min(fixed)),
            ] {
                let expected = subtotal - amount + i128::from(fee);
                let quoted = price_order(&lines, Some(discount), fee);
                if expected > i128::from(MAX_ORDER_TOTAL_CENTAVOS) {
                    assert_eq!(quoted, Err(PricingError::OrderLimit));
                } else {
                    let quoted = quoted.unwrap();
                    assert_eq!(i128::from(quoted.items_subtotal_centavos), subtotal);
                    assert_eq!(i128::from(quoted.discount_centavos), amount);
                    assert_eq!(i128::from(quoted.total_centavos), expected);
                    assert!(quoted.total_centavos >= quoted.delivery_fee_centavos);
                }
            }
        }
    }
}
