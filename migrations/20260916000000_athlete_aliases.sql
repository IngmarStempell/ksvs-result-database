CREATE TABLE IF NOT EXISTS athlete_aliases (
    id INTEGER PRIMARY KEY NOT NULL,
    athlete_id INTEGER NOT NULL REFERENCES athletes(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    source TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_athlete_aliases_active_alias
ON athlete_aliases(alias) WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_athlete_aliases_athlete_id
ON athlete_aliases(athlete_id);
