CREATE TABLE IF NOT EXISTS parser_runs (
    id INTEGER PRIMARY KEY NOT NULL,
    import_run_id INTEGER REFERENCES import_runs(id) ON DELETE SET NULL,
    source_name TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    parser_name TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    input_path TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    source_report_path TEXT,
    export_generated_at TEXT,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at TEXT,
    status TEXT NOT NULL,
    error TEXT,
    UNIQUE (source_name, input_hash, parser_name, parser_version)
);

CREATE TABLE IF NOT EXISTS parsed_result_rows (
    id INTEGER PRIMARY KEY NOT NULL,
    parser_run_id INTEGER NOT NULL REFERENCES parser_runs(id) ON DELETE CASCADE,
    source_document_id INTEGER REFERENCES source_documents(id) ON DELETE SET NULL,
    row_index INTEGER NOT NULL,
    row_fingerprint TEXT NOT NULL UNIQUE,
    canonical_fingerprint TEXT NOT NULL,
    source_name TEXT NOT NULL,
    competition_year INTEGER NOT NULL,
    competition_scope TEXT NOT NULL,
    result_kind TEXT NOT NULL,
    rank INTEGER,
    score REAL,
    raw_shooter_name TEXT,
    normalized_shooter_name TEXT,
    raw_club_name TEXT,
    normalized_club_name TEXT,
    association_code TEXT,
    raw_discipline TEXT,
    normalized_discipline TEXT,
    discipline_code TEXT,
    class_name TEXT,
    event_name TEXT,
    event_date TEXT,
    pdf_url TEXT,
    local_path TEXT,
    raw_payload TEXT,
    conflict_status TEXT NOT NULL DEFAULT 'none',
    conflict_result_id INTEGER REFERENCES results(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (parser_run_id, row_index)
);

ALTER TABLE results
ADD COLUMN parsed_result_row_id INTEGER REFERENCES parsed_result_rows(id) ON DELETE SET NULL;

ALTER TABLE results
ADD COLUMN canonical_fingerprint TEXT;

ALTER TABLE results
ADD COLUMN conflict_status TEXT NOT NULL DEFAULT 'none';

CREATE UNIQUE INDEX IF NOT EXISTS idx_results_canonical_fingerprint
ON results(canonical_fingerprint)
WHERE canonical_fingerprint IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_parser_runs_input_hash ON parser_runs(input_hash);
CREATE INDEX IF NOT EXISTS idx_parser_runs_source_name ON parser_runs(source_name);
CREATE INDEX IF NOT EXISTS idx_parsed_result_rows_parser_run_id
ON parsed_result_rows(parser_run_id);
CREATE INDEX IF NOT EXISTS idx_parsed_result_rows_canonical_fingerprint
ON parsed_result_rows(canonical_fingerprint);
CREATE INDEX IF NOT EXISTS idx_parsed_result_rows_conflict_status
ON parsed_result_rows(conflict_status);
