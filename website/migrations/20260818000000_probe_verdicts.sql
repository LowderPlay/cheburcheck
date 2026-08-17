ALTER TABLE probe_reports
    RENAME COLUMN verdict TO verdicts;

ALTER TABLE probe_reports
    ALTER COLUMN verdicts TYPE VARCHAR(32)[]
    USING ARRAY[verdicts];

UPDATE probe_reports
SET result = (result - 'verdict') || jsonb_build_object('verdicts', to_jsonb(verdicts));
