INSERT INTO probe_reports (
    query_id, query, probe_id, verdicts, result,
    target_hop_count, target_trace_result, duration_ms
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
