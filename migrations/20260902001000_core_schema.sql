CREATE TABLE IF NOT EXISTS source_documents (
    id INTEGER PRIMARY KEY NOT NULL,
    source_name TEXT NOT NULL,
    url TEXT NOT NULL,
    local_path TEXT,
    sha256 TEXT,
    etag TEXT,
    last_modified TEXT,
    file_size_bytes INTEGER,
    classification TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (url)
);

CREATE TABLE IF NOT EXISTS import_runs (
    id INTEGER PRIMARY KEY NOT NULL,
    source_document_id INTEGER REFERENCES source_documents(id) ON DELETE SET NULL,
    source_name TEXT NOT NULL,
    run_kind TEXT NOT NULL,
    parser_name TEXT,
    parser_version TEXT,
    input_path TEXT,
    input_hash TEXT,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at TEXT,
    status TEXT NOT NULL,
    error TEXT
);

CREATE TABLE IF NOT EXISTS competitions (
    id INTEGER PRIMARY KEY NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    year INTEGER NOT NULL,
    scope TEXT NOT NULL,
    organizer TEXT,
    association_code TEXT,
    country_code TEXT,
    date_from TEXT,
    date_to TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (code, year, name)
);

CREATE TABLE IF NOT EXISTS clubs (
    id INTEGER PRIMARY KEY NOT NULL,
    canonical_name TEXT NOT NULL UNIQUE,
    association_code TEXT,
    source TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS athletes (
    id INTEGER PRIMARY KEY NOT NULL,
    canonical_name TEXT NOT NULL UNIQUE,
    sort_name TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS disciplines (
    id INTEGER PRIMARY KEY NOT NULL,
    code TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (code, name)
);

CREATE TABLE IF NOT EXISTS results (
    id INTEGER PRIMARY KEY NOT NULL,
    import_run_id INTEGER REFERENCES import_runs(id) ON DELETE SET NULL,
    source_document_id INTEGER REFERENCES source_documents(id) ON DELETE SET NULL,
    competition_id INTEGER NOT NULL REFERENCES competitions(id) ON DELETE RESTRICT,
    athlete_id INTEGER REFERENCES athletes(id) ON DELETE SET NULL,
    club_id INTEGER REFERENCES clubs(id) ON DELETE SET NULL,
    discipline_id INTEGER REFERENCES disciplines(id) ON DELETE SET NULL,
    result_kind TEXT NOT NULL,
    rank INTEGER,
    score REAL,
    medal TEXT,
    participation_only INTEGER NOT NULL DEFAULT 0,
    event_class TEXT,
    stage TEXT NOT NULL DEFAULT 'unknown',
    raw_shooter_name TEXT,
    raw_club_name TEXT,
    raw_discipline TEXT,
    raw_payload TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (participation_only IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_source_documents_source_name ON source_documents(source_name);
CREATE INDEX IF NOT EXISTS idx_import_runs_source_name ON import_runs(source_name);
CREATE INDEX IF NOT EXISTS idx_competitions_year_scope ON competitions(year, scope);
CREATE INDEX IF NOT EXISTS idx_results_competition_id ON results(competition_id);
CREATE INDEX IF NOT EXISTS idx_results_athlete_id ON results(athlete_id);
CREATE INDEX IF NOT EXISTS idx_results_club_id ON results(club_id);
CREATE INDEX IF NOT EXISTS idx_results_discipline_id ON results(discipline_id);
