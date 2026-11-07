ALTER TABLE shared_results
    ADD COLUMN IF NOT EXISTS query_id UUID;

CREATE UNIQUE INDEX IF NOT EXISTS shared_results_query_id_idx
    ON shared_results (query_id);
