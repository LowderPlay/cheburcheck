CREATE INDEX IF NOT EXISTS queries_date_query_idx
    ON queries (date, query);

CREATE INDEX IF NOT EXISTS queries_date_source_ip_idx
    ON queries (date, source_ip);

CREATE INDEX IF NOT EXISTS queries_source_ip_date_idx
    ON queries (source_ip, date);
