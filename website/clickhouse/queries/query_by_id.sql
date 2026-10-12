SELECT query, resolved_ips
FROM queries
WHERE id = $1
LIMIT 1
