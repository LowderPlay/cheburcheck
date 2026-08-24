ALTER TABLE reporters
    ADD COLUMN IF NOT EXISTS last_connection_ip VARCHAR(45),
    ADD COLUMN IF NOT EXISTS last_connected_at TIMESTAMPTZ;
