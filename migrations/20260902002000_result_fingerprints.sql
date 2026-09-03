ALTER TABLE results
ADD COLUMN source_fingerprint TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_results_source_fingerprint
ON results(source_fingerprint)
WHERE source_fingerprint IS NOT NULL;
