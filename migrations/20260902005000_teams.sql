CREATE TABLE IF NOT EXISTS teams (
    id INTEGER PRIMARY KEY NOT NULL,
    competition_id INTEGER NOT NULL REFERENCES competitions(id) ON DELETE RESTRICT,
    club_id INTEGER REFERENCES clubs(id) ON DELETE SET NULL,
    discipline_id INTEGER REFERENCES disciplines(id) ON DELETE SET NULL,
    source_document_id INTEGER REFERENCES source_documents(id) ON DELETE SET NULL,
    parsed_result_row_id INTEGER REFERENCES parsed_result_rows(id) ON DELETE SET NULL,
    canonical_name TEXT NOT NULL,
    team_number TEXT,
    raw_team_name TEXT,
    rank INTEGER,
    score REAL,
    medal TEXT,
    event_class TEXT,
    source_fingerprint TEXT,
    canonical_fingerprint TEXT,
    conflict_status TEXT NOT NULL DEFAULT 'none',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_teams_source_fingerprint
ON teams(source_fingerprint)
WHERE source_fingerprint IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_teams_canonical_fingerprint
ON teams(canonical_fingerprint)
WHERE canonical_fingerprint IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_teams_competition_id
ON teams(competition_id);

CREATE INDEX IF NOT EXISTS idx_teams_club_id
ON teams(club_id);

CREATE TABLE IF NOT EXISTS team_members (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    athlete_id INTEGER REFERENCES athletes(id) ON DELETE SET NULL,
    member_order INTEGER NOT NULL,
    display_name TEXT NOT NULL,
    raw_name TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (team_id, athlete_id, member_order)
);

CREATE INDEX IF NOT EXISTS idx_team_members_team_id
ON team_members(team_id);

CREATE INDEX IF NOT EXISTS idx_team_members_athlete_id
ON team_members(athlete_id);

CREATE TABLE IF NOT EXISTS team_result_members (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    result_id INTEGER NOT NULL REFERENCES results(id) ON DELETE CASCADE,
    athlete_id INTEGER REFERENCES athletes(id) ON DELETE SET NULL,
    team_member_id INTEGER REFERENCES team_members(id) ON DELETE SET NULL,
    member_order INTEGER NOT NULL,
    score REAL,
    medal TEXT,
    raw_name TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (team_id, result_id)
);

CREATE INDEX IF NOT EXISTS idx_team_result_members_team_id
ON team_result_members(team_id);

CREATE INDEX IF NOT EXISTS idx_team_result_members_result_id
ON team_result_members(result_id);

CREATE INDEX IF NOT EXISTS idx_team_result_members_athlete_id
ON team_result_members(athlete_id);
