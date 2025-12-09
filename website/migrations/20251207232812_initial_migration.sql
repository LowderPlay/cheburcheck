-- Queries
CREATE TABLE IF NOT EXISTS queries
(
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    query                   VARCHAR(255) NOT NULL,
    source_ip               VARCHAR(39)  NOT NULL,
    source_country_code     VARCHAR(5),
    source_city_geo_name_id INT,

    target_country_code     VARCHAR(5),
    target_asn              VARCHAR(32),
    target_provider         VARCHAR(255),

    resolved_ips            VARCHAR(39)[],

    cdn_networks            VARCHAR(43)[],
    cdn_providers           VARCHAR(255)[],

    rkn_domain              VARCHAR(255),

    date                    TIMESTAMP        DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS queries_query_date_idx ON queries (query, date);

-- Human reports
CREATE TABLE IF NOT EXISTS human_reports
(
    id        UUID PRIMARY KEY REFERENCES queries (id),
    source_ip VARCHAR(39) NOT NULL,
    date      TIMESTAMP DEFAULT NOW(),
    works     BOOLEAN
);

CREATE INDEX IF NOT EXISTS human_reports_date_idx ON human_reports (date);
CREATE INDEX IF NOT EXISTS human_reports_works_idx ON human_reports (works);

-- Domain rankings list
CREATE TABLE IF NOT EXISTS domains
(
    domain VARCHAR(255) PRIMARY KEY,
    rank   INT
);

CREATE INDEX IF NOT EXISTS domains_domain_rank_idx ON domains (domain, rank);

-- Agency reporters
CREATE TABLE IF NOT EXISTS reporters
(
    id    SERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL,
    name  VARCHAR(255) NOT NULL
);

CREATE INDEX IF NOT EXISTS reporters_token_idx ON reporters (token);

-- Agency reports
CREATE TABLE IF NOT EXISTS reports
(
    id           SERIAL PRIMARY KEY,
    reporter     SERIAL,
    reporter_ip  VARCHAR(39) NOT NULL,
    date         TIMESTAMP DEFAULT NOW(),

    version      VARCHAR(32) NOT NULL,
    http         BOOLEAN,
    tx_junk      BOOLEAN,
    ip           VARCHAR(39),
    path         VARCHAR(255),
    retry_count  INT,
    timeout_secs INT,
    probe_count  INT,

    FOREIGN KEY (reporter) REFERENCES reporters (id)
);

CREATE INDEX IF NOT EXISTS reports_reporter_date_id_idx ON reports (reporter, date DESC, id);

DO
$$
    BEGIN
        CREATE TYPE evidence AS ENUM ('ok', 'blocked', 'connection_error', 'unknown_error');
    EXCEPTION
        WHEN duplicate_object THEN null;
    END
$$;

CREATE TABLE IF NOT EXISTS report_row
(
    id        BIGSERIAL PRIMARY KEY,
    report_id SERIAL,
    domain    VARCHAR(255),
    evidence  evidence,
    FOREIGN KEY (report_id) REFERENCES reports (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS report_row_id_domain_evidence_idx ON report_row (report_id, domain, evidence);

-- Whitelist view
CREATE MATERIALIZED VIEW IF NOT EXISTS
    whitelist AS
WITH ranked_reports AS (SELECT rr.domain,
                               rr.evidence,
                               r.date,
                               ROW_NUMBER() OVER (
                                   PARTITION BY
                                       rr.domain
                                   ORDER BY
                                       r.date DESC
                                   ) AS rn
                        FROM report_row rr
                                 JOIN reports r ON rr.report_id = r.id
                        WHERE r.reporter = 1)
SELECT rr.domain,
       d.rank,
       MAX(
               CASE
                   WHEN rr.evidence = 'ok' THEN rr.date
                   END
       ) AS last_ok
FROM ranked_reports rr
         LEFT JOIN domains d ON d.domain = rr.domain
WHERE rr.rn <= 5
GROUP BY rr.domain,
         d.rank
HAVING COUNT(*) FILTER (
    WHERE
    rr.evidence = 'ok'
    ) >= COUNT(*) / 2.0
ORDER BY d.rank;
