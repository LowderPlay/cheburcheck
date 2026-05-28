ALTER TABLE
    queries
ALTER COLUMN
    id
SET
    DEFAULT uuidv7();

CREATE INDEX IF NOT EXISTS reports_reporter_date_id_desc_idx ON reports (reporter, date DESC, id DESC);

CREATE INDEX IF NOT EXISTS report_row_domain_idx ON report_row (domain);

DROP MATERIALIZED VIEW IF EXISTS whitelist;

CREATE MATERIALIZED VIEW whitelist AS WITH report_domains AS (
    SELECT
        DISTINCT domain
    FROM
        report_row
)
SELECT
    latest.domain,
    d.rank,
    MAX(
        CASE
            WHEN latest.evidence = 'ok' THEN latest.date
        END
    ) AS last_ok
FROM
    report_domains rd
    CROSS JOIN LATERAL (
        SELECT
            rd.domain,
            rr.evidence,
            r.date
        FROM
            reports r
            JOIN report_row rr ON rr.report_id = r.id
            AND rr.domain = rd.domain
        WHERE
            r.reporter = 1
        ORDER BY
            r.date DESC,
            r.id DESC
        LIMIT
            5
    ) latest
    LEFT JOIN domains d ON d.domain = latest.domain
GROUP BY
    latest.domain,
    d.rank
HAVING
    COUNT(*) FILTER (
        WHERE
            latest.evidence = 'ok'
    ) >= COUNT(*) / 2.0
ORDER BY
    d.rank;

CREATE UNIQUE INDEX IF NOT EXISTS whitelist_domain_idx ON whitelist (domain);

CREATE INDEX IF NOT EXISTS whitelist_rank_idx ON whitelist (rank);