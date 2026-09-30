-- Keep historical reports when their reporter is removed.
ALTER TABLE reports ALTER COLUMN reporter DROP NOT NULL;
