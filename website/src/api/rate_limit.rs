use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use std::net::IpAddr;
use std::num::NonZeroU32;

pub type ApiRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

pub fn build_rate_limiter(per_minute: u32) -> ApiRateLimiter {
    RateLimiter::keyed(Quota::per_minute(
        NonZeroU32::new(per_minute).expect("rate limit must be > 0"),
    ))
}
