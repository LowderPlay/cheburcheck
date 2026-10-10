CREATE TABLE IF NOT EXISTS queries
(
    id UUID DEFAULT generateUUIDv4(),
    query String,
    query_lower String MATERIALIZED lowerUTF8(query),
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
    date Nullable(DateTime64(6, 'UTC')) DEFAULT now64(6),
    PROJECTION by_id (SELECT * ORDER BY id),
    PROJECTION by_date (SELECT * ORDER BY (ifNull(date, toDateTime64(0, 6, 'UTC')), id))
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ifNull(date, toDateTime64(0, 6, 'UTC')))
ORDER BY (query_lower, ifNull(date, toDateTime64(0, 6, 'UTC')), id);

CREATE TABLE IF NOT EXISTS probe_reports
(
    id UUID DEFAULT generateUUIDv4(),
    postgres_id Nullable(Int64),
    query_id UUID,
    query String,
    query_lower String MATERIALIZED lowerUTF8(query),
    probe_id Int32,
    date Nullable(DateTime64(6, 'UTC')) DEFAULT now64(6),
    verdicts Array(Nullable(String)),
    result String CODEC(ZSTD(3)),
    target_hop_count Nullable(Int16),
    target_trace_result Nullable(String),
    duration_ms Nullable(Int64),
    INDEX query_id_idx query_id TYPE bloom_filter(0.01) GRANULARITY 1,
    PROJECTION by_probe
    (
        SELECT probe_id, date, query_id, query_lower, verdicts,
               target_hop_count, target_trace_result, duration_ms
        ORDER BY (probe_id, ifNull(date, toDateTime64(0, 6, 'UTC')), query_id)
    )
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ifNull(date, toDateTime64(0, 6, 'UTC')))
ORDER BY (query_lower, ifNull(date, toDateTime64(0, 6, 'UTC')), probe_id, query_id);

CREATE TABLE IF NOT EXISTS human_reports
(
    id UUID DEFAULT generateUUIDv4(),
    query_id UUID,
    query String,
    query_lower String MATERIALIZED lowerUTF8(query),
    source_ip String,
    date Nullable(DateTime64(6, 'UTC')) DEFAULT now64(6),
    works Nullable(Bool),
    INDEX query_id_idx query_id TYPE bloom_filter(0.01) GRANULARITY 1
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(ifNull(date, toDateTime64(0, 6, 'UTC')))
ORDER BY (query_lower, ifNull(date, toDateTime64(0, 6, 'UTC')), query_id, id);
