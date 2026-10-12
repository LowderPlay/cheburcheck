INSERT INTO probe_reports
    (postgres_id, query_id, query, probe_id, date, verdicts, result,
     target_hop_count, target_trace_result, duration_ms)
SELECT
    p.id, p.query_id, q.query, p.probe_id,
    if(isNull(p.date), NULL,
       toTimeZone(parseDateTime64BestEffort(ifNull(p.date, '1970-01-01 00:00:00'),
                                          6, {source_timezone:String}), 'UTC')),
    p.verdicts, p.result, p.target_hop_count, p.target_trace_result, p.duration_ms
FROM backfill_pg_probe_reports AS p
INNER JOIN queries AS q ON q.id = p.query_id
SETTINGS external_table_functions_use_nulls = 1;
