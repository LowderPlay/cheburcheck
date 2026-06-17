use log::{info, warn};
use querying::Checker;
use querying::cache::DatabaseCache;
use rocket::tokio;
use rocket::tokio::sync::RwLock;
use rocket::tokio::time;
use std::sync::Arc;
use std::time::Duration;

pub async fn start() -> Arc<RwLock<Checker>> {
    let database_interval = Duration::from_secs(
        std::env::var("DATABASE_INTERVAL_SECONDS")
            .unwrap_or("21600".to_string())
            .parse()
            .unwrap(),
    );
    let database_retry_interval = Duration::from_secs(
        std::env::var("DATABASE_RETRY_INTERVAL_SECONDS")
            .unwrap_or("300".to_string())
            .parse()
            .unwrap(),
    );

    let cache = DatabaseCache::from_env();
    let checker = Checker::new().await;
    let first_refresh_delay = match cache.load() {
        Ok(bases) => {
            info!("Loading databases from {}", cache.path().display());
            checker.update_all(bases).await;
            info!("Loaded cached databases");
            cache
                .refresh_delay(database_interval)
                .unwrap_or(Duration::ZERO)
        }
        Err(e) => {
            warn!(
                "Failed to load cached databases from {}: {}",
                cache.path().display(),
                e
            );
            Duration::ZERO
        }
    };
    let checker = Arc::new(RwLock::new(checker));

    spawn_refresh_loop(
        checker.clone(),
        cache,
        database_interval,
        database_retry_interval,
        first_refresh_delay,
    );

    checker
}

fn spawn_refresh_loop(
    checker: Arc<RwLock<Checker>>,
    cache: DatabaseCache,
    database_interval: Duration,
    database_retry_interval: Duration,
    first_refresh_delay: Duration,
) {
    tokio::spawn(async move {
        info!(
            "Refreshing DB every {:?}; retrying failures after {:?}",
            database_interval, database_retry_interval
        );
        if !first_refresh_delay.is_zero() {
            info!("Next DB refresh in {:?}", first_refresh_delay);
            time::sleep(first_refresh_delay).await;
        }
        loop {
            info!("Updating all DBs");
            let next_refresh = match Checker::download_all().await {
                Ok(bases) => {
                    info!("Downloaded, updating...");
                    if let Err(e) = cache.store(&bases) {
                        log::error!("Failed to write database cache: {}", e);
                    }
                    checker.read().await.update_all(bases).await;
                    info!("Updated databases");
                    database_interval
                }
                Err(e) => {
                    log::error!("Failed to download all DBs: {}", e);
                    database_retry_interval
                }
            };
            time::sleep(next_refresh).await;
        }
    });
}
