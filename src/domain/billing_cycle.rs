//! Renewal schedule for providers without native recurrence (Banco Inter): one charge per
//! cycle, issued `ISSUE_LEAD_DAYS` before the cycle's due date. Pure functions.

use chrono::{Duration, NaiveDate};
use sha2::{Digest, Sha256};

use crate::domain::affiliate_math::add_months_floor;

/// Days before the due date when the next cycle's charge is issued.
pub const ISSUE_LEAD_DAYS: i64 = 10;
/// Days the first charge stays payable after its due date (Inter `numDiasAgenda`).
pub const PAYABLE_DAYS_AFTER_DUE: i64 = 5;

/// Due date of cycle `index`, always computed from the first due date so month-end anchors do
/// not drift (Jan 31 -> Feb 28 -> Mar 31), matching the coverage rule of `billing_coverage`.
pub fn cycle_due_date(anchor: NaiveDate, months: i64, index: i64) -> NaiveDate {
    let offset = u32::try_from(months * index).expect("cycle offset fits in u32");
    add_months_floor(anchor, offset)
}

/// Deterministic Inter `seuNumero` (max 15 chars) for a cycle, so a retried request after a
/// timeout reuses the same number instead of creating a second charge.
pub fn our_number(reference: &str, cycle_index: i64) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let digest = Sha256::digest(format!("{reference}:{cycle_index}").as_bytes());
    let mut value = u128::from_be_bytes(digest[..16].try_into().expect("16-byte slice"));
    let mut out = String::from("Q");
    while out.len() < 15 {
        out.push(char::from(ALPHABET[(value % 36) as usize]));
        value /= 36;
    }
    out
}

/// Inter charge statuses that settle the cycle.
pub fn is_paid(status: &str) -> bool {
    matches!(status, "RECEBIDO" | "MARCADO_RECEBIDO")
}

/// Statuses after which a charge can no longer be paid.
pub fn is_dead(status: &str) -> bool {
    matches!(status, "CANCELADO" | "EXPIRADO" | "FALHA_EMISSAO")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenewalAction {
    /// Nothing to do yet.
    Wait,
    /// Issue the charge of `cycle_index` due on `due_date`.
    Issue {
        cycle_index: i64,
        due_date: NaiveDate,
    },
    /// The last cycle was not paid and can no longer be: stop renewing.
    Lapse,
}

/// Decides what the scheduler does for one subscription today.
pub fn next_action(
    anchor: NaiveDate,
    months: i64,
    last_cycle: i64,
    last_status: &str,
    today: NaiveDate,
) -> RenewalAction {
    if is_dead(last_status) {
        return RenewalAction::Lapse;
    }
    if !is_paid(last_status) {
        return RenewalAction::Wait;
    }
    let next = last_cycle + 1;
    let due = cycle_due_date(anchor, months, next);
    if today >= due - Duration::days(ISSUE_LEAD_DAYS) {
        RenewalAction::Issue {
            cycle_index: next,
            due_date: due,
        }
    } else {
        RenewalAction::Wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(raw: &str) -> NaiveDate {
        raw.parse().unwrap()
    }

    #[test]
    fn cycles_follow_the_anchor_without_drift() {
        let anchor = day("2026-01-31");
        assert_eq!(cycle_due_date(anchor, 1, 1), day("2026-02-28"));
        assert_eq!(cycle_due_date(anchor, 1, 2), day("2026-03-31"));
        assert_eq!(cycle_due_date(anchor, 6, 1), day("2026-07-31"));
        assert_eq!(cycle_due_date(day("2024-02-29"), 12, 1), day("2025-02-28"));
    }

    #[test]
    fn our_number_is_short_stable_and_distinct() {
        let a = our_number("0193a0c0-0000-7000-8000-000000000001", 0);
        assert_eq!(a.len(), 15);
        assert!(a.starts_with('Q') && a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_eq!(a, our_number("0193a0c0-0000-7000-8000-000000000001", 0));
        assert_ne!(a, our_number("0193a0c0-0000-7000-8000-000000000001", 1));
    }

    #[test]
    fn renewal_waits_for_payment_and_issues_ahead_of_due_date() {
        let anchor = day("2026-01-10");
        assert_eq!(
            next_action(anchor, 1, 0, "A_RECEBER", day("2026-02-05")),
            RenewalAction::Wait
        );
        assert_eq!(
            next_action(anchor, 1, 0, "RECEBIDO", day("2026-01-30")),
            RenewalAction::Wait
        );
        assert_eq!(
            next_action(anchor, 1, 0, "RECEBIDO", day("2026-01-31")),
            RenewalAction::Issue {
                cycle_index: 1,
                due_date: day("2026-02-10")
            }
        );
        assert_eq!(
            next_action(anchor, 1, 3, "EXPIRADO", day("2026-05-01")),
            RenewalAction::Lapse
        );
    }
}
