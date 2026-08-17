ALTER TABLE probe_reports
    ADD COLUMN target_hop_count SMALLINT,
    ADD COLUMN target_trace_result VARCHAR(32),
    ADD COLUMN control_hop_count SMALLINT,
    ADD COLUMN control_trace_result VARCHAR(32);
