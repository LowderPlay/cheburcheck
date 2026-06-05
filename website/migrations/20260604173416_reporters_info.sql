ALTER TABLE
    reporters
ADD
    COLUMN IF NOT EXISTS region VARCHAR(255),
ADD
    COLUMN IF NOT EXISTS asn VARCHAR(32),
ADD
    COLUMN IF NOT EXISTS provider VARCHAR(255);

CREATE TABLE IF NOT EXISTS probe_reports (
    id BIGSERIAL PRIMARY KEY,
    query_id UUID NOT NULL REFERENCES queries (id) ON DELETE CASCADE,
    probe_id INT NOT NULL REFERENCES reporters (id) ON DELETE CASCADE,
    date TIMESTAMP DEFAULT NOW(),
    verdict VARCHAR(32) NOT NULL,
    result JSONB NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS probe_reports_query_probe_idx ON probe_reports (query_id, probe_id);

CREATE INDEX IF NOT EXISTS probe_reports_probe_date_idx ON probe_reports (probe_id, date DESC);