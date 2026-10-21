ALTER TABLE queries ADD COLUMN IF NOT EXISTS user_agent Nullable(String) AFTER source_ip;
