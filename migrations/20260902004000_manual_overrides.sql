CREATE TABLE IF NOT EXISTS manual_overrides (
    id INTEGER PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL DEFAULT 'global',
    entity_type TEXT NOT NULL,
    entity_id INTEGER,
    source_document_id INTEGER REFERENCES source_documents(id) ON DELETE SET NULL,
    parsed_result_row_id INTEGER REFERENCES parsed_result_rows(id) ON DELETE SET NULL,
    field_name TEXT NOT NULL,
    old_value TEXT NOT NULL,
    new_value TEXT NOT NULL,
    reason TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_manual_overrides_active_value
ON manual_overrides(entity_type, field_name, old_value, scope)
WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_manual_overrides_entity
ON manual_overrides(entity_type, field_name, status);

CREATE INDEX IF NOT EXISTS idx_manual_overrides_source_document_id
ON manual_overrides(source_document_id);

CREATE INDEX IF NOT EXISTS idx_manual_overrides_parsed_result_row_id
ON manual_overrides(parsed_result_row_id);
