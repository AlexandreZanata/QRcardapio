use askama::Template;

use crate::domain::affiliate::{
    AffiliateStatus, CommissionStatus, CommissionType, EntryKind, ReferralStatus,
    format_commission_rate,
};
use crate::i18n::{Locale, format_price};

use super::PageMeta;

pub struct AffiliateSummaryView {
    pub id: String,
    pub account_email: String,
    pub code: String,
    pub status_key: &'static str,
    pub status_badge: &'static str,
    pub quota_display: String,
    pub quota_unit_key: &'static str,
    pub hold_days: i64,
    pub click_count: i64,
    pub referrals_total: i64,
    pub referrals_active: i64,
    pub pending_formatted: String,
    pub available_formatted: String,
    pub paid_formatted: String,
}

pub struct ReferralView {
    pub id: String,
    pub account_label: String,
    pub establishment_name: String,
    pub establishment_slug: String,
    pub has_establishment: bool,
    pub status_key: &'static str,
    pub status_badge: &'static str,
    pub can_disqualify: bool,
    pub disqualified_reason: String,
    pub attributed_at: String,
    pub first_paid_at: Option<String>,
}

pub struct CommissionView {
    pub payment_id: String,
    pub establishment_name: String,
    pub kind_key: &'static str,
    pub is_clawback: bool,
    pub installment_label: String,
    pub gross_formatted: String,
    pub commission_formatted: String,
    pub rate_snapshot: String,
    pub status_key: &'static str,
    pub status_badge: &'static str,
    pub available_on: String,
}

pub struct PayoutView {
    pub id: String,
    pub total_formatted: String,
    pub entries_count: i64,
    pub pix_key_snapshot: String,
    pub reference: String,
    pub notes: String,
    pub paid_by_email: String,
    pub paid_at: String,
}

#[derive(Template)]
#[template(path = "dashboard/affiliate.html")]
pub struct AffiliatePortalTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub billing_env: String,
    pub self_signup_enabled: bool,
    pub default_quota_display: String,
    pub default_quota_unit_key: &'static str,
    pub default_hold_days: i64,
    pub cookie_ttl_days: i64,
    pub is_enrolled: bool,
    pub referral_url: String,
    pub code: String,
    pub status_key: &'static str,
    pub status_badge: &'static str,
    pub quota_display: String,
    pub quota_unit_key: &'static str,
    pub hold_days: i64,
    pub pix_key: String,
    pub click_count: i64,
    pub referrals_total: i64,
    pub referrals_active: i64,
    pub pending_formatted: String,
    pub available_formatted: String,
    pub paid_formatted: String,
    pub referrals: Vec<ReferralView>,
    pub commissions: Vec<CommissionView>,
    pub payouts: Vec<PayoutView>,
}

#[derive(Template)]
#[template(path = "dashboard/admin_affiliates.html")]
pub struct AdminAffiliatesTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub billing_env: String,
    pub self_signup_enabled: bool,
    pub default_commission_type: String,
    pub default_commission_input: String,
    pub default_hold_days: i64,
    pub cookie_ttl_days: i64,
    pub total_affiliates: i64,
    pub active_affiliates: i64,
    pub active_referrals: i64,
    pub pending_formatted: String,
    pub available_formatted: String,
    pub paid_formatted: String,
    pub affiliates: Vec<AffiliateSummaryView>,
}

#[derive(Template)]
#[template(path = "dashboard/admin_affiliate_detail.html")]
pub struct AdminAffiliateDetailTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub billing_env: String,
    pub affiliate_id: String,
    pub account_email: String,
    pub code: String,
    pub referral_url: String,
    pub status: String,
    pub status_key: &'static str,
    pub status_badge: &'static str,
    pub commission_type: String,
    pub commission_input: String,
    pub quota_display: String,
    pub quota_unit_key: &'static str,
    pub hold_days: i64,
    pub pix_key: String,
    pub notes: String,
    pub click_count: i64,
    pub referrals_total: i64,
    pub referrals_active: i64,
    pub pending_formatted: String,
    pub available_formatted: String,
    pub paid_formatted: String,
    pub voided_formatted: String,
    pub can_settle_payout: bool,
    pub referrals: Vec<ReferralView>,
    pub commissions: Vec<CommissionView>,
    pub payouts: Vec<PayoutView>,
}

pub fn affiliate_status_meta(status: AffiliateStatus) -> (&'static str, &'static str) {
    match status {
        AffiliateStatus::Active => ("affiliate.status_active", "badge-success"),
        AffiliateStatus::Paused => ("affiliate.status_paused", "badge-warning"),
        AffiliateStatus::Revoked => ("affiliate.status_revoked", "badge-danger"),
    }
}

pub fn referral_status_meta(status: ReferralStatus) -> (&'static str, &'static str) {
    match status {
        ReferralStatus::SignedUp => ("affiliate.ref_status_signed_up", "badge-neutral"),
        ReferralStatus::Active => ("affiliate.ref_status_active", "badge-success"),
        ReferralStatus::Disqualified => ("affiliate.ref_status_disqualified", "badge-danger"),
    }
}

pub fn commission_status_meta(status: CommissionStatus) -> (&'static str, &'static str) {
    match status {
        CommissionStatus::Pending => ("affiliate.comm_status_pending", "badge-warning"),
        CommissionStatus::Available => ("affiliate.comm_status_available", "badge-info"),
        CommissionStatus::Paid => ("affiliate.comm_status_paid", "badge-success"),
        CommissionStatus::Voided => ("affiliate.comm_status_voided", "badge-neutral"),
    }
}

pub fn commission_unit_key(kind: CommissionType) -> &'static str {
    match kind {
        CommissionType::FixedMonthlyCentavos => "affiliate.unit_per_month",
        CommissionType::PercentBps => "affiliate.unit_percent_monthly",
    }
}

pub fn map_commission_view(
    row: crate::infrastructure::database::queries::affiliate_ledger::CommissionLedgerRow,
    locale: Locale,
) -> CommissionView {
    let (status_key, status_badge) = commission_status_meta(row.effective_status);
    let is_clawback = row.entry_kind == EntryKind::Clawback;
    CommissionView {
        payment_id: row.payment_id,
        establishment_name: row.establishment_name,
        kind_key: if is_clawback {
            "affiliate.entry_clawback"
        } else {
            "affiliate.entry_commission"
        },
        is_clawback,
        installment_label: format!("{}/{}", row.installment_index, row.billing_cycle_months),
        gross_formatted: format_price(row.gross_payment_centavos, locale),
        commission_formatted: format_price(row.commission_centavos, locale),
        rate_snapshot: format_commission_rate(
            row.snapshot_commission_type,
            row.snapshot_commission_value,
            locale,
        ),
        status_key,
        status_badge,
        available_on: row.available_on,
    }
}

pub fn map_payout_view(
    row: crate::infrastructure::database::queries::affiliate_ledger::PayoutRow,
    locale: Locale,
) -> PayoutView {
    PayoutView {
        id: row.id,
        total_formatted: format_price(row.total_centavos, locale),
        entries_count: row.entries_count,
        pix_key_snapshot: row.pix_key_snapshot,
        reference: row.reference,
        notes: row.notes,
        paid_by_email: row.paid_by_email,
        paid_at: row.paid_at,
    }
}
