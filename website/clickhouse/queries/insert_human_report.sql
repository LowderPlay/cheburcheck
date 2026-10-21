INSERT INTO human_reports (query_id, query, source_ip, works)
SETTINGS async_insert = 1, wait_for_async_insert = 1,
    async_insert_use_adaptive_busy_timeout = 0, async_insert_busy_timeout_ms = 100
VALUES ($1, $2, $3, $4)
