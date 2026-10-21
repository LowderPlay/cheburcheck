SELECT query, resolved_ips, user_agent
FROM queries
WHERE id = $1
LIMIT 1
