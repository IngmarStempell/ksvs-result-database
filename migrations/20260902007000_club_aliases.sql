CREATE TABLE IF NOT EXISTS club_aliases (
    id INTEGER PRIMARY KEY NOT NULL,
    club_id INTEGER NOT NULL REFERENCES clubs(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    association_code TEXT,
    source TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_club_aliases_active_alias
ON club_aliases(alias)
WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_club_aliases_club_id
ON club_aliases(club_id);

CREATE INDEX IF NOT EXISTS idx_club_aliases_association_code
ON club_aliases(association_code);
