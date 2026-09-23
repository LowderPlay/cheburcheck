ALTER TABLE probe_reports
    ADD COLUMN duration_ms BIGINT CHECK (duration_ms >= 0);
