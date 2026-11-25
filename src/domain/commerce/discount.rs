//! Discounts take a non-negative item subtotal and never discount delivery fees.

use super::pricing::PricingError;

const BASIS_POINTS: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discount {
    BasisPoints(i64),
    FixedCentavos(i64),
}

impl Discount {
    pub fn amount(self, items_subtotal_centavos: i64) -> Result<i64, PricingError> {
        if items_subtotal_centavos < 0 {
            return Err(PricingError::NegativeAmount);
        }
        match self {
            Self::BasisPoints(bps) if (0..=BASIS_POINTS).contains(&bps) => {
                // This rounds the discount itself half-up, as required for Commerce Core.
                let numerator = i128::from(items_subtotal_centavos)
                    .checked_mul(i128::from(bps))
                    .and_then(|value| value.checked_add(i128::from(BASIS_POINTS / 2)))
                    .ok_or(PricingError::Overflow)?;
                i64::try_from(numerator / i128::from(BASIS_POINTS))
                    .map_err(|_| PricingError::Overflow)
            }
            Self::FixedCentavos(amount) if amount >= 0 => Ok(amount.min(items_subtotal_centavos)),
            _ => Err(PricingError::InvalidDiscount),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentages_round_half_up_and_fixed_discounts_stop_at_the_subtotal() {
        assert_eq!(Discount::BasisPoints(5_000).amount(1), Ok(1));
        assert_eq!(Discount::BasisPoints(4_999).amount(1), Ok(0));
        assert_eq!(Discount::BasisPoints(5_000).amount(3), Ok(2));
        assert_eq!(Discount::BasisPoints(10_000).amount(i64::MAX), Ok(i64::MAX));
        assert_eq!(Discount::FixedCentavos(i64::MAX).amount(9), Ok(9));
        assert_eq!(
            Discount::BasisPoints(10_001).amount(1),
            Err(PricingError::InvalidDiscount)
        );
        assert_eq!(
            Discount::FixedCentavos(-1).amount(1),
            Err(PricingError::InvalidDiscount)
        );
        assert_eq!(
            Discount::BasisPoints(500).amount(-1),
            Err(PricingError::NegativeAmount)
        );
    }
}
