CREATE TABLE IF NOT EXISTS organizations (
    id INTEGER PRIMARY KEY NOT NULL,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    organization_type TEXT NOT NULL,
    parent_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
    country_code TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (code, organization_type)
);

CREATE TABLE IF NOT EXISTS organization_aliases (
    id INTEGER PRIMARY KEY NOT NULL,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    source TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_organization_aliases_active_alias
ON organization_aliases(alias)
WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_organizations_parent_id
ON organizations(parent_id);

ALTER TABLE competitions
ADD COLUMN organizer_organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL;

ALTER TABLE results
ADD COLUMN representing_organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL;

ALTER TABLE results
ADD COLUMN start_context TEXT NOT NULL DEFAULT 'club';

INSERT INTO organizations (code, name, organization_type, country_code)
VALUES
    ('OD', 'Kreisschuetzenverband Stormarn', 'district_association', 'DE'),
    ('NDSB', 'Norddeutscher Schuetzenbund', 'state_association', 'DE'),
    ('DSB', 'Deutscher Schuetzenbund', 'national_association', 'DE'),
    ('DE', 'Deutschland', 'nation', 'DE'),
    ('ISSF', 'International Shooting Sport Federation', 'international_federation', NULL),
    ('IOC', 'International Olympic Committee', 'olympic_organization', NULL)
ON CONFLICT(code, organization_type) DO UPDATE SET
    name = excluded.name,
    country_code = excluded.country_code;

INSERT INTO organization_aliases (organization_id, alias, source, status)
SELECT id, code, 'baseline', 'active'
FROM organizations
WHERE TRUE
ON CONFLICT(alias) WHERE status = 'active'
DO UPDATE SET organization_id = excluded.organization_id;
