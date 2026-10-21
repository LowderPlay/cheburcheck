INSERT INTO probe_reports (
    query_id, query, probe_id, verdicts, result,
    target_hop_count, target_trace_result, duration_ms
) SETTINGS async_insert = 1, wait_for_async_insert = 0,
    async_insert_use_adaptive_busy_timeout = 0, async_insert_busy_timeout_ms = 100
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
