ALTER TABLE probe_api_keys
    ADD COLUMN credits BIGINT NOT NULL DEFAULT 0 CHECK (credits >= 0);
