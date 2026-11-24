use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};

type KeyedLimiter<K> = RateLimiter<K, DashMapStateStore<K>, DefaultClock>;

/// Bounded per-IP and per-account GCRA rate limiters for abuse-sensitive endpoints.
#[derive(Clone)]
pub struct AppRateLimiters {
    pub mcp_oauth_ip: Arc<KeyedLimiter<IpAddr>>,
    pub mcp_establishment: Arc<KeyedLimiter<String>>,
    pub google_ip: Arc<KeyedLimiter<IpAddr>>,
    pub login_ip: Arc<KeyedLimiter<IpAddr>>,
    pub register_ip: Arc<KeyedLimiter<IpAddr>>,
    pub reset_ip: Arc<KeyedLimiter<IpAddr>>,
    pub upload_account: Arc<KeyedLimiter<String>>,
    pub ocr_account: Arc<KeyedLimiter<String>>,
    pub billing_establishment: Arc<KeyedLimiter<String>>,
    pub tool_qr_ip: Arc<KeyedLimiter<IpAddr>>,
    pub affiliate_click_ip: Arc<KeyedLimiter<IpAddr>>,
}

impl Default for AppRateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRateLimiters {
    pub fn new() -> Self {
        Self {
            mcp_oauth_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(120).expect("non-zero"),
            ))),
            mcp_establishment: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(120).expect("non-zero"),
            ))),
            google_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(12).expect("non-zero"),
            ))),
            login_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(10).expect("non-zero"),
            ))),
            register_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(5).expect("non-zero"),
            ))),
            reset_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(5).expect("non-zero"),
            ))),
            upload_account: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(20).expect("non-zero"),
            ))),
            ocr_account: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(10).expect("non-zero"),
            ))),
            billing_establishment: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(6).expect("non-zero"),
            ))),
            tool_qr_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(60).expect("non-zero"),
            ))),
            affiliate_click_ip: Arc::new(RateLimiter::dashmap(Quota::per_minute(
                NonZeroU32::new(60).expect("non-zero"),
            ))),
        }
    }

    /// Reclaims stale limiter buckets so memory stays bounded on small VPS hosts.
    pub fn prune_stale(&self) {
        self.mcp_oauth_ip.retain_recent();
        self.mcp_oauth_ip.shrink_to_fit();
        self.mcp_establishment.retain_recent();
        self.mcp_establishment.shrink_to_fit();
        self.google_ip.retain_recent();
        self.google_ip.shrink_to_fit();
        self.login_ip.retain_recent();
        self.login_ip.shrink_to_fit();
        self.register_ip.retain_recent();
        self.register_ip.shrink_to_fit();
        self.reset_ip.retain_recent();
        self.reset_ip.shrink_to_fit();
        self.upload_account.retain_recent();
        self.upload_account.shrink_to_fit();
        self.ocr_account.retain_recent();
        self.ocr_account.shrink_to_fit();
        self.billing_establishment.retain_recent();
        self.billing_establishment.shrink_to_fit();
        self.tool_qr_ip.retain_recent();
        self.tool_qr_ip.shrink_to_fit();
        self.affiliate_click_ip.retain_recent();
        self.affiliate_click_ip.shrink_to_fit();
    }
}
