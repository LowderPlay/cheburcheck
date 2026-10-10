-- Configure the backfill_postgres named collection on the ClickHouse server first.
-- These are remote table mappings, not copies. Never INSERT into them.
-- Read timestamps as text so PostgreSQL TIMESTAMP wall-clock values can be
-- interpreted explicitly in source_timezone without losing microseconds.
CREATE TABLE backfill_pg_queries
(
    id UUID,
    query String,
    source_ip String,
    source_country_code Nullable(String),
    source_city_geo_name_id Nullable(Int32),
    target_country_code Nullable(String),
    target_asn Nullable(String),
    target_provider Nullable(String),
    resolved_ips Array(Nullable(String)),
    cdn_networks Array(Nullable(String)),
    cdn_providers Array(Nullable(String)),
    rkn_domain Nullable(String),
    date Nullable(String)
)
ENGINE = PostgreSQL(backfill_postgres, table = 'queries');

CREATE TABLE backfill_pg_probe_reports
(
    id Int64,
    query_id UUID,
    probe_id Int32,
    date Nullable(String),
    verdicts Array(Nullable(String)),
    result String,
    target_hop_count Nullable(Int16),
    target_trace_result Nullable(String),
    duration_ms Nullable(Int64)
)
ENGINE = PostgreSQL(backfill_postgres, table = 'probe_reports');

CREATE TABLE backfill_pg_human_reports
(
    id UUID,
    source_ip String,
    date Nullable(String),
    works Nullable(Bool)
)
ENGINE = PostgreSQL(backfill_postgres, table = 'human_reports');
