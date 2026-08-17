use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use std::net::IpAddr;
use std::num::NonZeroU32;

type KeyedRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

pub struct ApiRateLimiter(KeyedRateLimiter);

pub struct ProbeRateLimiter(KeyedRateLimiter);

impl ApiRateLimiter {
    pub fn check(&self, ip: &IpAddr) -> bool {
        self.0.check_key(ip).is_ok()
    }
}

impl ProbeRateLimiter {
    pub fn check(&self, ip: &IpAddr) -> bool {
        self.0.check_key(ip).is_ok()
    }
}

pub fn build_rate_limiter(per_minute: u32) -> ApiRateLimiter {
    ApiRateLimiter(build_limiter(per_minute))
}

pub fn build_probe_rate_limiter(per_minute: u32) -> ProbeRateLimiter {
    ProbeRateLimiter(build_limiter(per_minute))
}

fn build_limiter(per_minute: u32) -> KeyedRateLimiter {
    RateLimiter::keyed(Quota::per_minute(
        NonZeroU32::new(per_minute).expect("rate limit must be > 0"),
    ))
}
