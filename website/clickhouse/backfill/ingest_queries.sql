INSERT INTO queries
    (id, query, source_ip, source_country_code, source_city_geo_name_id,
     target_country_code, target_asn, target_provider, resolved_ips,
     cdn_networks, cdn_providers, rkn_domain, date)
SELECT
    id, query, source_ip, source_country_code, source_city_geo_name_id,
    target_country_code, target_asn, target_provider, resolved_ips,
    cdn_networks, cdn_providers, rkn_domain,
    if(isNull(date), NULL,
       toTimeZone(parseDateTime64BestEffort(ifNull(date, '1970-01-01 00:00:00'),
                                          6, {source_timezone:String}), 'UTC'))
FROM backfill_pg_queries
SETTINGS external_table_functions_use_nulls = 1;
