ALTER TABLE reporters
    ADD COLUMN IF NOT EXISTS last_connected_at TIMESTAMPTZ;
