INSERT INTO queries (
    id, query, source_ip, source_country_code, source_city_geo_name_id,
    target_country_code, target_asn, target_provider, resolved_ips,
    cdn_networks, cdn_providers, rkn_domain
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
