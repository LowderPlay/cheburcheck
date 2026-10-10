-- Removes only the ClickHouse mappings; the PostgreSQL tables are untouched.
DROP TABLE IF EXISTS backfill_pg_human_reports;
DROP TABLE IF EXISTS backfill_pg_probe_reports;
DROP TABLE IF EXISTS backfill_pg_queries;
