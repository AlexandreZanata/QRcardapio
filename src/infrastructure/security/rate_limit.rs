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
    pub login_ip: Arc<KeyedLimiter<IpAddr>>,
    pub register_ip: Arc<KeyedLimiter<IpAddr>>,
    pub reset_ip: Arc<KeyedLimiter<IpAddr>>,
    pub upload_account: Arc<KeyedLimiter<String>>,
}

impl Default for AppRateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRateLimiters {
    pub fn new() -> Self {
        Self {
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
        }
    }

    /// Reclaims stale limiter buckets so memory stays bounded on small VPS hosts.
    pub fn prune_stale(&self) {
        self.login_ip.retain_recent();
        self.login_ip.shrink_to_fit();
        self.register_ip.retain_recent();
        self.register_ip.shrink_to_fit();
        self.reset_ip.retain_recent();
        self.reset_ip.shrink_to_fit();
        self.upload_account.retain_recent();
        self.upload_account.shrink_to_fit();
    }
}
