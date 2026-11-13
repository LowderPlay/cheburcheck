use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use std::net::IpAddr;
use std::num::NonZeroU32;

type KeyedRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

pub struct ApiRateLimiter(KeyedRateLimiter);

pub struct ProbeRateLimiter {
    default: KeyedRateLimiter,
    credited: RateLimiter<uuid::Uuid, DefaultKeyedStateStore<uuid::Uuid>, DefaultClock>,
}

pub struct ProbeUpdateDownloadRateLimiter(KeyedRateLimiter);

impl ApiRateLimiter {
    pub fn check(&self, ip: &IpAddr) -> bool {
        self.0.check_key(ip).is_ok()
    }
}

impl ProbeRateLimiter {
    // Lock the balance until admission is decided so concurrent requests cannot
    // spend the same last credit. Rejected requests do not spend credits.
    pub async fn admit(
        &self,
        pool: &sqlx::PgPool,
        token_id: Option<uuid::Uuid>,
        ip: &IpAddr,
    ) -> Result<(), rocket::http::Status> {
        use rocket::http::Status;
        let Some(token_id) = token_id else {
            return self
                .default
                .check_key(ip)
                .map_err(|_| Status::TooManyRequests);
        };
        let unavailable = |error| {
            log::warn!("api: failed to debit probe API credits: {error}");
            Status::ServiceUnavailable
        };
        let mut tx = pool.begin().await.map_err(unavailable)?;
        let credits = sqlx::query_scalar::<_, i64>(
            "SELECT credits FROM probe_api_keys WHERE id = $1
             AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()) FOR UPDATE",
        )
        .bind(token_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?
        .ok_or(Status::Unauthorized)?;
        if credits > 0 {
            self.credited
                .check_key(&token_id)
                .map_err(|_| Status::TooManyRequests)?;
            sqlx::query("UPDATE probe_api_keys SET credits = credits - 1 WHERE id = $1")
                .bind(token_id)
                .execute(&mut *tx)
                .await
                .map_err(unavailable)?;
        } else {
            self.default
                .check_key(ip)
                .map_err(|_| Status::TooManyRequests)?;
        }
        tx.commit().await.map_err(unavailable)?;
        Ok(())
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

pub fn build_probe_rate_limiter(
    per_minute: u32,
    burst: u32,
    credit_per_minute: u32,
    credit_burst: u32,
) -> ProbeRateLimiter {
    ProbeRateLimiter {
        default: build_limiter(per_minute, burst),
        credited: RateLimiter::keyed(quota(credit_per_minute, credit_burst)),
    }
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

    #[rocket::async_test]
    async fn default_admission_does_not_need_database() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        let limiter = build_probe_rate_limiter(5, 1, 60, 10);
        let ip = "192.0.2.1".parse().unwrap();
        assert_eq!(limiter.admit(&pool, None, &ip).await, Ok(()));
        assert_eq!(
            limiter.admit(&pool, None, &ip).await,
            Err(rocket::http::Status::TooManyRequests)
        );
    }

    #[rocket::async_test]
    #[ignore = "requires disposable PostgreSQL in PROBE_AUTH_TEST_DATABASE_URL"]
    async fn postgres_credits_admission_topups_and_concurrency() {
        use crate::admin::{Admin, ProbeApiCreditsInput, add_probe_api_credits};
        use rocket::http::Status;
        use rocket::serde::json::Json;
        let url = std::env::var("PROBE_AUTH_TEST_DATABASE_URL").unwrap();
        let admin = sqlx::PgPool::connect(&url).await.unwrap();
        let schema = format!("probe_credit_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let search_path = schema.clone();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .after_connect(move |conn, _| {
                let sql = format!("SET search_path TO {search_path}");
                Box::pin(async move {
                    sqlx::query(&sql).execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261105000000_probe_api_keys.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261114000000_probe_api_credits.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO probe_api_keys(name, key_hash) VALUES ('credits', repeat('a',64)) RETURNING id"
        ).fetch_one(&pool).await.unwrap();
        let balance = || async {
            sqlx::query_scalar::<_, i64>("SELECT credits FROM probe_api_keys WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        };
        assert_eq!(balance().await, 0);
        let ip = "192.0.2.1".parse().unwrap();
        let limiter = build_probe_rate_limiter(5, 1, 60, 2);
        assert_eq!(limiter.admit(&pool, Some(id), &ip).await, Ok(()));
        assert_eq!(
            limiter.admit(&pool, Some(id), &ip).await,
            Err(Status::TooManyRequests)
        );
        for credits in [0, -1, i64::MAX] {
            assert_eq!(
                add_probe_api_credits(
                    Admin,
                    &id.to_string(),
                    Json(ProbeApiCreditsInput { credits }),
                    rocket::State::from(&pool)
                )
                .await
                .err(),
                Some(Status::BadRequest)
            );
        }
        let topped = add_probe_api_credits(
            Admin,
            &id.to_string(),
            Json(ProbeApiCreditsInput { credits: 3 }),
            rocket::State::from(&pool),
        )
        .await
        .unwrap()
        .0
        .into_inner();
        assert_eq!(topped.credits, 3);
        // Credited requests bypass the exhausted IP bucket; their own limit still applies.
        for _ in 0..2 {
            assert_eq!(limiter.admit(&pool, Some(id), &ip).await, Ok(()));
        }
        assert_eq!(
            limiter
                .admit(&pool, Some(id), &"192.0.2.2".parse().unwrap())
                .await,
            Err(Status::TooManyRequests)
        );
        assert_eq!(balance().await, 1);
        // Many concurrent calls compete for the final credit. Only one can spend it;
        // one more can use the default bucket, and the rest must be rejected.
        let limiter = std::sync::Arc::new(build_probe_rate_limiter(5, 1, 60, 100));
        let mut calls = rocket::tokio::task::JoinSet::new();
        for _ in 0..20 {
            let limiter = limiter.clone();
            let pool = pool.clone();
            calls.spawn(async move { limiter.admit(&pool, Some(id), &ip).await });
        }
        let mut results = Vec::new();
        while let Some(result) = calls.join_next().await {
            results.push(result.unwrap());
        }
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 2);
        assert!(
            results
                .iter()
                .all(|result| *result == Ok(()) || *result == Err(Status::TooManyRequests))
        );
        assert_eq!(balance().await, 0);
        // Atomic additions must also survive concurrency with debits.
        let first = add_probe_api_credits(
            Admin,
            &id.to_string(),
            Json(ProbeApiCreditsInput { credits: 10 }),
            rocket::State::from(&pool),
        )
        .await
        .unwrap();
        assert_eq!(first.0.credits, 10);
        let id_string = id.to_string();
        let (added, spent) = rocket::tokio::join!(
            add_probe_api_credits(
                Admin,
                &id_string,
                Json(ProbeApiCreditsInput { credits: 10 }),
                rocket::State::from(&pool)
            ),
            limiter.admit(&pool, Some(id), &ip)
        );
        assert!(added.is_ok());
        assert_eq!(spent, Ok(()));
        assert_eq!(balance().await, 19);
        sqlx::query("UPDATE probe_api_keys SET revoked_at=now() WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            limiter.admit(&pool, Some(id), &ip).await,
            Err(Status::Unauthorized)
        );
        assert_eq!(balance().await, 19);
        pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&admin)
            .await
            .unwrap();
    }

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
