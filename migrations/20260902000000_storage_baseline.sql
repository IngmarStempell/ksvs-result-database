CREATE TABLE IF NOT EXISTS app_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO app_metadata (key, value)
VALUES ('schema_purpose', 'pdf-explorer storage baseline')
ON CONFLICT(key) DO NOTHING;
