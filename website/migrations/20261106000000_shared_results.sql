CREATE TABLE IF NOT EXISTS shared_results (
    id UUID PRIMARY KEY,
    snapshot JSONB NOT NULL CHECK (jsonb_typeof(snapshot) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS shared_results_expires_at_idx
    ON shared_results (expires_at);
