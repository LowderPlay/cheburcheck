INSERT INTO human_reports
    (query_id, query, source_ip, date, works)
SELECT
    h.id, q.query, h.source_ip,
    if(isNull(h.date), NULL,
       toTimeZone(parseDateTime64BestEffort(ifNull(h.date, '1970-01-01 00:00:00'),
                                          6, {source_timezone:String}), 'UTC')),
    h.works
FROM backfill_pg_human_reports AS h
INNER JOIN queries AS q ON q.id = h.id
SETTINGS external_table_functions_use_nulls = 1;
