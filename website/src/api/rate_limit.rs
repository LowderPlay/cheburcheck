use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use std::net::IpAddr;
use std::num::NonZeroU32;

type KeyedRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

pub struct ApiRateLimiter(KeyedRateLimiter);

pub struct ProbeRateLimiter(KeyedRateLimiter);

pub struct ProbeUpdateDownloadRateLimiter(KeyedRateLimiter);

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

impl ProbeUpdateDownloadRateLimiter {
    pub fn check(&self, ip: &IpAddr) -> bool {
        self.0.check_key(ip).is_ok()
    }
}

pub fn build_rate_limiter(per_minute: u32, burst: u32) -> ApiRateLimiter {
    ApiRateLimiter(build_limiter(per_minute, burst))
}

pub fn build_probe_rate_limiter(per_minute: u32, burst: u32) -> ProbeRateLimiter {
    ProbeRateLimiter(build_limiter(per_minute, burst))
}

pub fn build_probe_update_download_rate_limiter(
    per_minute: u32,
    burst: u32,
) -> ProbeUpdateDownloadRateLimiter {
    ProbeUpdateDownloadRateLimiter(build_limiter(per_minute, burst))
}

fn build_limiter(per_minute: u32, burst: u32) -> KeyedRateLimiter {
    RateLimiter::keyed(quota(per_minute, burst))
}

fn quota(per_minute: u32, burst: u32) -> Quota {
    Quota::per_minute(NonZeroU32::new(per_minute).expect("rate limit must be > 0"))
        .allow_burst(NonZeroU32::new(burst).expect("rate limit burst must be > 0"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use governor::clock::FakeRelativeClock;
    use std::time::Duration;

    #[test]
    fn requests_are_spaced_per_ip_without_accumulating_a_burst() {
        let clock = FakeRelativeClock::default();
        let limiter = RateLimiter::dashmap_with_clock(quota(5, 1), clock.clone());
        let first: IpAddr = "192.0.2.1".parse().unwrap();
        let second: IpAddr = "192.0.2.2".parse().unwrap();
        assert!(limiter.check_key(&first).is_ok());
        assert!(limiter.check_key(&first).is_err());
        assert!(limiter.check_key(&second).is_ok());
        clock.advance(Duration::from_secs(11));
        assert!(limiter.check_key(&first).is_err());
        clock.advance(Duration::from_secs(1));
        assert!(limiter.check_key(&first).is_ok());
        assert!(limiter.check_key(&first).is_err());
        clock.advance(Duration::from_secs(60));
        assert!(limiter.check_key(&first).is_ok());
        assert!(limiter.check_key(&first).is_err());
    }

    #[test]
    fn configurable_burst_preserves_the_replenishment_rate() {
        let clock = FakeRelativeClock::default();
        let limiter = RateLimiter::dashmap_with_clock(quota(5, 3), clock.clone());
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        for _ in 0..3 {
            assert!(limiter.check_key(&ip).is_ok());
        }
        assert!(limiter.check_key(&ip).is_err());
        clock.advance(Duration::from_secs(12));
        assert!(limiter.check_key(&ip).is_ok());
        assert!(limiter.check_key(&ip).is_err());
        clock.advance(Duration::from_secs(60));
        for _ in 0..3 {
            assert!(limiter.check_key(&ip).is_ok());
        }
        assert!(limiter.check_key(&ip).is_err());
    }
}
