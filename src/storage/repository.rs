use anyhow::{Result, bail};
use sqlx::SqlitePool;

use super::models::{
    CanonicalResultReference, ClubAlias, ManualOverride, NewAthlete, NewClub, NewClubAlias,
    NewCompetition, NewDiscipline, NewImportRun, NewManualOverride, NewOrganization,
    NewOrganizationAlias, NewParsedResultRow, NewParserRun, NewResult, NewSourceDocument, NewTeam,
    NewTeamMember, NewTeamResultMember, StorageCounts, StoredParsedResultRow, StoredResult,
};

pub struct StorageRepository<'a> {
    pub(super) pool: &'a SqlitePool,
}

#[derive(sqlx::FromRow)]
struct ManualOverrideRow {
    id: i64,
    scope: String,
    entity_type: String,
    field_name: String,
    old_value: String,
    new_value: String,
    reason: Option<String>,
    status: String,
}

#[derive(sqlx::FromRow)]
struct ClubAliasRow {
    id: i64,
    club_id: i64,
    alias: String,
    canonical_name: String,
    association_code: Option<String>,
    source: Option<String>,
    status: String,
}

impl From<ClubAliasRow> for ClubAlias {
    fn from(row: ClubAliasRow) -> Self {
        Self {
            id: row.id,
            club_id: row.club_id,
            alias: row.alias,
            canonical_name: row.canonical_name,
            association_code: row.association_code,
            source: row.source,
            status: row.status,
        }
    }
}

impl From<ManualOverrideRow> for ManualOverride {
    fn from(row: ManualOverrideRow) -> Self {
        Self {
            id: row.id,
            scope: row.scope,
            entity_type: row.entity_type,
            field_name: row.field_name,
            old_value: row.old_value,
            new_value: row.new_value,
            reason: row.reason,
            status: row.status,
        }
    }
}

impl<'a> StorageRepository<'a> {
    #[must_use]
    pub const fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Stores a source document and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or lookup fails.
    pub async fn upsert_source_document(&self, document: &NewSourceDocument) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO source_documents (
                source_name, url, local_path, sha256, etag, last_modified,
                file_size_bytes, classification
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(url) DO UPDATE SET
                source_name = excluded.source_name,
                local_path = excluded.local_path,
                etag = excluded.etag,
                last_modified = excluded.last_modified,
                file_size_bytes = excluded.file_size_bytes,
                classification = excluded.classification
            RETURNING id
            ",
        )
        .bind(&document.source_name)
        .bind(&document.url)
        .bind(&document.local_path)
        .bind(&document.sha256)
        .bind(&document.etag)
        .bind(&document.last_modified)
        .bind(document.file_size_bytes)
        .bind(&document.classification)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores an import or parser run and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn insert_import_run(&self, run: &NewImportRun) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO import_runs (
                source_document_id, source_name, run_kind, parser_name,
                parser_version, input_path, input_hash, status, error
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
        )
        .bind(run.source_document_id)
        .bind(&run.source_name)
        .bind(&run.run_kind)
        .bind(&run.parser_name)
        .bind(&run.parser_version)
        .bind(&run.input_path)
        .bind(&run.input_hash)
        .bind(&run.status)
        .bind(&run.error)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or reuses a parser run and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn upsert_parser_run(&self, run: &NewParserRun) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO parser_runs (
                import_run_id, source_name, source_kind, parser_name,
                parser_version, input_path, input_hash, source_report_path,
                export_generated_at, status, error
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(source_name, input_hash, parser_name, parser_version) DO UPDATE SET
                import_run_id = excluded.import_run_id,
                source_kind = excluded.source_kind,
                input_path = excluded.input_path,
                source_report_path = excluded.source_report_path,
                export_generated_at = excluded.export_generated_at,
                status = excluded.status,
                error = excluded.error
            RETURNING id
            ",
        )
        .bind(run.import_run_id)
        .bind(&run.source_name)
        .bind(&run.source_kind)
        .bind(&run.parser_name)
        .bind(&run.parser_version)
        .bind(&run.input_path)
        .bind(&run.input_hash)
        .bind(&run.source_report_path)
        .bind(&run.export_generated_at)
        .bind(&run.status)
        .bind(&run.error)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or finds a competition and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or lookup fails.
    pub async fn upsert_competition(&self, competition: &NewCompetition) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO competitions (
                code, name, year, scope, organizer, association_code,
                country_code, date_from, date_to
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(code, year, name) DO UPDATE SET
                scope = excluded.scope,
                organizer = excluded.organizer,
                association_code = excluded.association_code,
                country_code = excluded.country_code,
                date_from = excluded.date_from,
                date_to = excluded.date_to
            RETURNING id
            ",
        )
        .bind(&competition.code)
        .bind(&competition.name)
        .bind(competition.year)
        .bind(&competition.scope)
        .bind(&competition.organizer)
        .bind(&competition.association_code)
        .bind(&competition.country_code)
        .bind(&competition.date_from)
        .bind(&competition.date_to)
        .fetch_one(self.pool)
        .await?;

        if let Some(organizer_code) = competition_organizer_code(competition) {
            sqlx::query(
                r"
                UPDATE competitions
                SET organizer_organization_id = (
                    SELECT id
                    FROM organizations
                    WHERE code = ?
                    ORDER BY id
                    LIMIT 1
                )
                WHERE id = ?
                ",
            )
            .bind(organizer_code)
            .bind(id)
            .execute(self.pool)
            .await?;
        }

        Ok(id)
    }

    /// Stores or finds a canonical club and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or lookup fails.
    pub async fn upsert_club(&self, club: &NewClub) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO clubs (canonical_name, association_code, source)
            VALUES (?, ?, ?)
            ON CONFLICT(canonical_name) DO UPDATE SET
                association_code = COALESCE(excluded.association_code, clubs.association_code),
                source = COALESCE(excluded.source, clubs.source)
            RETURNING id
            ",
        )
        .bind(&club.canonical_name)
        .bind(&club.association_code)
        .bind(&club.source)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or updates an active alias for a canonical club.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn upsert_club_alias(&self, alias: &NewClubAlias) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO club_aliases (club_id, alias, association_code, source, status)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(alias) WHERE status = 'active'
            DO UPDATE SET
                club_id = excluded.club_id,
                association_code = COALESCE(excluded.association_code, club_aliases.association_code),
                source = COALESCE(excluded.source, club_aliases.source),
                updated_at = CURRENT_TIMESTAMP
            RETURNING id
            ",
        )
        .bind(alias.club_id)
        .bind(&alias.alias)
        .bind(&alias.association_code)
        .bind(&alias.source)
        .bind(&alias.status)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Reads active club aliases mapped to canonical club names.
    ///
    /// # Errors
    ///
    /// Returns an error when the database lookup fails.
    pub async fn active_club_aliases(&self) -> Result<Vec<ClubAlias>> {
        let rows = sqlx::query_as::<_, ClubAliasRow>(
            r"
            SELECT
                club_aliases.id,
                club_aliases.club_id,
                club_aliases.alias,
                clubs.canonical_name,
                club_aliases.association_code,
                club_aliases.source,
                club_aliases.status
            FROM club_aliases
            JOIN clubs ON clubs.id = club_aliases.club_id
            WHERE club_aliases.status = 'active'
            ORDER BY club_aliases.alias
            ",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(ClubAlias::from).collect())
    }

    /// Reads all club aliases including inactive rows.
    ///
    /// # Errors
    ///
    /// Returns an error when the database lookup fails.
    pub async fn all_club_aliases(&self) -> Result<Vec<ClubAlias>> {
        let rows = sqlx::query_as::<_, ClubAliasRow>(
            r"
            SELECT
                club_aliases.id,
                club_aliases.club_id,
                club_aliases.alias,
                clubs.canonical_name,
                club_aliases.association_code,
                club_aliases.source,
                club_aliases.status
            FROM club_aliases
            JOIN clubs ON clubs.id = club_aliases.club_id
            ORDER BY clubs.canonical_name, club_aliases.alias
            ",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(ClubAlias::from).collect())
    }

    /// Marks a club alias as inactive.
    ///
    /// # Errors
    ///
    /// Returns an error when the database update fails.
    pub async fn deactivate_club_alias(&self, id: i64) -> Result<()> {
        sqlx::query(
            r"
            UPDATE club_aliases
            SET status = 'inactive', updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            ",
        )
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Looks up a club by its canonical name.
    ///
    /// # Errors
    ///
    /// Returns an error when the database lookup fails.
    pub async fn find_club_id_by_name(&self, canonical_name: &str) -> Result<Option<i64>> {
        sqlx::query_scalar::<_, i64>("SELECT id FROM clubs WHERE canonical_name = ?")
            .bind(canonical_name)
            .fetch_optional(self.pool)
            .await
            .map_err(Into::into)
    }

    /// Stores or finds an organization and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or lookup fails.
    pub async fn upsert_organization(&self, organization: &NewOrganization) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO organizations (code, name, organization_type, parent_id, country_code)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(code, organization_type) DO UPDATE SET
                name = excluded.name,
                parent_id = excluded.parent_id,
                country_code = excluded.country_code
            RETURNING id
            ",
        )
        .bind(&organization.code)
        .bind(&organization.name)
        .bind(&organization.organization_type)
        .bind(organization.parent_id)
        .bind(&organization.country_code)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or updates an active organization alias.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn upsert_organization_alias(&self, alias: &NewOrganizationAlias) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO organization_aliases (organization_id, alias, source, status)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(alias) WHERE status = 'active'
            DO UPDATE SET
                organization_id = excluded.organization_id,
                source = COALESCE(excluded.source, organization_aliases.source),
                updated_at = CURRENT_TIMESTAMP
            RETURNING id
            ",
        )
        .bind(alias.organization_id)
        .bind(&alias.alias)
        .bind(&alias.source)
        .bind(&alias.status)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or finds a canonical athlete and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or lookup fails.
    pub async fn upsert_athlete(&self, athlete: &NewAthlete) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO athletes (canonical_name, sort_name)
            VALUES (?, ?)
            ON CONFLICT(canonical_name) DO UPDATE SET
                sort_name = COALESCE(excluded.sort_name, athletes.sort_name)
            RETURNING id
            ",
        )
        .bind(&athlete.canonical_name)
        .bind(&athlete.sort_name)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or finds a discipline and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or lookup fails.
    pub async fn upsert_discipline(&self, discipline: &NewDiscipline) -> Result<i64> {
        let code = discipline.code.as_deref().unwrap_or_default();
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO disciplines (code, name)
            VALUES (?, ?)
            ON CONFLICT(code, name) DO UPDATE SET
                name = excluded.name
            RETURNING id
            ",
        )
        .bind(code)
        .bind(&discipline.name)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or updates an active manual correction.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn upsert_manual_override(&self, manual_override: &NewManualOverride) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO manual_overrides (
                scope, entity_type, entity_id, source_document_id, parsed_result_row_id,
                field_name, old_value, new_value, reason, status
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(entity_type, field_name, old_value, scope) WHERE status = 'active'
            DO UPDATE SET
                entity_id = excluded.entity_id,
                source_document_id = excluded.source_document_id,
                parsed_result_row_id = excluded.parsed_result_row_id,
                new_value = excluded.new_value,
                reason = excluded.reason,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id
            ",
        )
        .bind(&manual_override.scope)
        .bind(&manual_override.entity_type)
        .bind(manual_override.entity_id)
        .bind(manual_override.source_document_id)
        .bind(manual_override.parsed_result_row_id)
        .bind(&manual_override.field_name)
        .bind(&manual_override.old_value)
        .bind(&manual_override.new_value)
        .bind(&manual_override.reason)
        .bind(&manual_override.status)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Reads active manual corrections for an entity field.
    ///
    /// # Errors
    ///
    /// Returns an error when the database lookup fails.
    pub async fn active_manual_overrides(
        &self,
        entity_type: &str,
        field_name: &str,
    ) -> Result<Vec<ManualOverride>> {
        let rows = sqlx::query_as::<_, ManualOverrideRow>(
            r"
            SELECT id, scope, entity_type, field_name, old_value, new_value, reason, status
            FROM manual_overrides
            WHERE entity_type = ? AND field_name = ? AND status = 'active'
            ORDER BY scope, old_value
            ",
        )
        .bind(entity_type)
        .bind(field_name)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(ManualOverride::from).collect())
    }

    /// Reads all active manual corrections.
    ///
    /// # Errors
    ///
    /// Returns an error when the database lookup fails.
    pub async fn all_active_manual_overrides(&self) -> Result<Vec<ManualOverride>> {
        let rows = sqlx::query_as::<_, ManualOverrideRow>(
            r"
            SELECT id, scope, entity_type, field_name, old_value, new_value, reason, status
            FROM manual_overrides
            WHERE status = 'active'
            ORDER BY entity_type, field_name, old_value
            ",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(ManualOverride::from).collect())
    }

    /// Stores a canonical result and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn insert_result(&self, result: &NewResult) -> Result<i64> {
        Ok(self.insert_result_once(result).await?.id)
    }

    /// Stores a canonical result, reporting whether it was newly inserted.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or duplicate lookup fails.
    pub async fn insert_result_once(&self, result: &NewResult) -> Result<StoredResult> {
        if result.source_fingerprint.is_none() {
            return Ok(StoredResult {
                id: self.insert_result_unchecked(result).await?,
                inserted: true,
            });
        }

        let participation_only = i64::from(result.participation_only);
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT OR IGNORE INTO results (
                import_run_id, source_document_id, competition_id, athlete_id,
                club_id, discipline_id, result_kind, rank, score, medal,
                participation_only, event_class, stage, raw_shooter_name,
                raw_club_name, raw_discipline, raw_payload, source_fingerprint,
                parsed_result_row_id, canonical_fingerprint, conflict_status
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
        )
        .bind(result.import_run_id)
        .bind(result.source_document_id)
        .bind(result.competition_id)
        .bind(result.athlete_id)
        .bind(result.club_id)
        .bind(result.discipline_id)
        .bind(&result.result_kind)
        .bind(result.rank)
        .bind(result.score)
        .bind(&result.medal)
        .bind(participation_only)
        .bind(&result.event_class)
        .bind(&result.stage)
        .bind(&result.raw_shooter_name)
        .bind(&result.raw_club_name)
        .bind(&result.raw_discipline)
        .bind(&result.raw_payload)
        .bind(&result.source_fingerprint)
        .bind(result.parsed_result_row_id)
        .bind(&result.canonical_fingerprint)
        .bind(&result.conflict_status)
        .fetch_optional(self.pool)
        .await?;

        if let Some(id) = id {
            return Ok(StoredResult { id, inserted: true });
        }

        let Some(fingerprint) = result.source_fingerprint.as_ref() else {
            bail!("missing source fingerprint after duplicate insert was ignored");
        };
        let existing_id = self
            .find_result_id_by_fingerprint(fingerprint, result.canonical_fingerprint.as_deref())
            .await?;
        Ok(StoredResult {
            id: existing_id,
            inserted: false,
        })
    }

    async fn insert_result_unchecked(&self, result: &NewResult) -> Result<i64> {
        let participation_only = i64::from(result.participation_only);
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO results (
                import_run_id, source_document_id, competition_id, athlete_id,
                club_id, discipline_id, result_kind, rank, score, medal,
                participation_only, event_class, stage, raw_shooter_name,
                raw_club_name, raw_discipline, raw_payload, source_fingerprint,
                parsed_result_row_id, canonical_fingerprint, conflict_status
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
        )
        .bind(result.import_run_id)
        .bind(result.source_document_id)
        .bind(result.competition_id)
        .bind(result.athlete_id)
        .bind(result.club_id)
        .bind(result.discipline_id)
        .bind(&result.result_kind)
        .bind(result.rank)
        .bind(result.score)
        .bind(&result.medal)
        .bind(participation_only)
        .bind(&result.event_class)
        .bind(&result.stage)
        .bind(&result.raw_shooter_name)
        .bind(&result.raw_club_name)
        .bind(&result.raw_discipline)
        .bind(&result.raw_payload)
        .bind(&result.source_fingerprint)
        .bind(result.parsed_result_row_id)
        .bind(&result.canonical_fingerprint)
        .bind(&result.conflict_status)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    /// Stores or reuses a team and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or duplicate lookup fails.
    pub async fn upsert_team(&self, team: &NewTeam) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO teams (
                competition_id, club_id, discipline_id, source_document_id,
                parsed_result_row_id, canonical_name, team_number, raw_team_name,
                rank, score, medal, event_class, source_fingerprint,
                canonical_fingerprint, conflict_status
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(canonical_fingerprint) WHERE canonical_fingerprint IS NOT NULL
            DO UPDATE SET
                club_id = COALESCE(excluded.club_id, teams.club_id),
                discipline_id = COALESCE(excluded.discipline_id, teams.discipline_id),
                source_document_id = COALESCE(excluded.source_document_id, teams.source_document_id),
                parsed_result_row_id = COALESCE(excluded.parsed_result_row_id, teams.parsed_result_row_id),
                canonical_name = excluded.canonical_name,
                team_number = excluded.team_number,
                raw_team_name = excluded.raw_team_name,
                rank = excluded.rank,
                score = excluded.score,
                medal = excluded.medal,
                event_class = excluded.event_class,
                source_fingerprint = COALESCE(teams.source_fingerprint, excluded.source_fingerprint),
                conflict_status = excluded.conflict_status
            RETURNING id
            ",
        )
        .bind(team.competition_id)
        .bind(team.club_id)
        .bind(team.discipline_id)
        .bind(team.source_document_id)
        .bind(team.parsed_result_row_id)
        .bind(&team.canonical_name)
        .bind(&team.team_number)
        .bind(&team.raw_team_name)
        .bind(team.rank)
        .bind(team.score)
        .bind(&team.medal)
        .bind(&team.event_class)
        .bind(&team.source_fingerprint)
        .bind(&team.canonical_fingerprint)
        .bind(&team.conflict_status)
        .fetch_one(self.pool)
        .await?;

        Ok(id)
    }

    /// Stores or reuses a team member and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or duplicate lookup fails.
    pub async fn upsert_team_member(&self, member: &NewTeamMember) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT OR IGNORE INTO team_members (
                team_id, athlete_id, member_order, display_name, raw_name
            )
            VALUES (?, ?, ?, ?, ?)
            RETURNING id
            ",
        )
        .bind(member.team_id)
        .bind(member.athlete_id)
        .bind(member.member_order)
        .bind(&member.display_name)
        .bind(&member.raw_name)
        .fetch_optional(self.pool)
        .await?;

        if let Some(id) = id {
            return Ok(id);
        }

        let id = sqlx::query_scalar::<_, i64>(
            r"
            SELECT id
            FROM team_members
            WHERE team_id = ? AND athlete_id = ? AND member_order = ?
            ",
        )
        .bind(member.team_id)
        .bind(member.athlete_id)
        .bind(member.member_order)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    /// Stores a member's result relation to a team.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or duplicate lookup fails.
    pub async fn upsert_team_result_member(&self, member: &NewTeamResultMember) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT OR IGNORE INTO team_result_members (
                team_id, result_id, athlete_id, team_member_id, member_order,
                score, medal, raw_name
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
        )
        .bind(member.team_id)
        .bind(member.result_id)
        .bind(member.athlete_id)
        .bind(member.team_member_id)
        .bind(member.member_order)
        .bind(member.score)
        .bind(&member.medal)
        .bind(&member.raw_name)
        .fetch_optional(self.pool)
        .await?;

        if let Some(id) = id {
            return Ok(id);
        }

        let id = sqlx::query_scalar::<_, i64>(
            r"
            SELECT id
            FROM team_result_members
            WHERE team_id = ? AND result_id = ?
            ",
        )
        .bind(member.team_id)
        .bind(member.result_id)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    /// Stores a parsed row, reporting whether it was newly inserted.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write or duplicate lookup fails.
    pub async fn insert_parsed_result_row_once(
        &self,
        row: &NewParsedResultRow,
    ) -> Result<StoredParsedResultRow> {
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT OR IGNORE INTO parsed_result_rows (
                parser_run_id, source_document_id, row_index, row_fingerprint,
                canonical_fingerprint, source_name, competition_year, competition_scope,
                result_kind, rank, score, raw_shooter_name, normalized_shooter_name,
                raw_club_name, normalized_club_name, association_code, raw_discipline,
                normalized_discipline, discipline_code, class_name, event_name, event_date,
                pdf_url, local_path, raw_payload, conflict_status, conflict_result_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
        )
        .bind(row.parser_run_id)
        .bind(row.source_document_id)
        .bind(row.row_index)
        .bind(&row.row_fingerprint)
        .bind(&row.canonical_fingerprint)
        .bind(&row.source_name)
        .bind(row.competition_year)
        .bind(&row.competition_scope)
        .bind(&row.result_kind)
        .bind(row.rank)
        .bind(row.score)
        .bind(&row.raw_shooter_name)
        .bind(&row.normalized_shooter_name)
        .bind(&row.raw_club_name)
        .bind(&row.normalized_club_name)
        .bind(&row.association_code)
        .bind(&row.raw_discipline)
        .bind(&row.normalized_discipline)
        .bind(&row.discipline_code)
        .bind(&row.class_name)
        .bind(&row.event_name)
        .bind(&row.event_date)
        .bind(&row.pdf_url)
        .bind(&row.local_path)
        .bind(&row.raw_payload)
        .bind(&row.conflict_status)
        .bind(row.conflict_result_id)
        .fetch_optional(self.pool)
        .await?;

        if let Some(id) = id {
            return Ok(StoredParsedResultRow { id, inserted: true });
        }

        let existing_id = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM parsed_result_rows WHERE row_fingerprint = ?",
        )
        .bind(&row.row_fingerprint)
        .fetch_one(self.pool)
        .await?;
        Ok(StoredParsedResultRow {
            id: existing_id,
            inserted: false,
        })
    }

    /// Finds an existing canonical result for a logical parser row.
    ///
    /// # Errors
    ///
    /// Returns an error when the lookup fails.
    pub async fn find_canonical_result(
        &self,
        canonical_fingerprint: &str,
    ) -> Result<Option<CanonicalResultReference>> {
        let row = sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
            r"
            SELECT id, source_fingerprint, raw_payload
            FROM results
            WHERE canonical_fingerprint = ?
            ",
        )
        .bind(canonical_fingerprint)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(
            |(id, source_fingerprint, raw_payload)| CanonicalResultReference {
                id,
                source_fingerprint,
                raw_payload,
            },
        ))
    }

    async fn find_result_id_by_fingerprint(
        &self,
        source_fingerprint: &str,
        canonical_fingerprint: Option<&str>,
    ) -> Result<i64> {
        if let Some(canonical_fingerprint) = canonical_fingerprint {
            let id = sqlx::query_scalar::<_, i64>(
                r"
                SELECT id
                FROM results
                WHERE source_fingerprint = ? OR canonical_fingerprint = ?
                ",
            )
            .bind(source_fingerprint)
            .bind(canonical_fingerprint)
            .fetch_one(self.pool)
            .await?;
            return Ok(id);
        }

        let id =
            sqlx::query_scalar::<_, i64>("SELECT id FROM results WHERE source_fingerprint = ?")
                .bind(source_fingerprint)
                .fetch_one(self.pool)
                .await?;
        Ok(id)
    }

    /// Counts the core storage tables.
    ///
    /// # Errors
    ///
    /// Returns an error when one of the count queries fails.
    pub async fn counts(&self) -> Result<StorageCounts> {
        Ok(StorageCounts {
            source_documents: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM source_documents")
                .fetch_one(self.pool)
                .await?,
            import_runs: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM import_runs")
                .fetch_one(self.pool)
                .await?,
            competitions: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM competitions")
                .fetch_one(self.pool)
                .await?,
            clubs: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clubs")
                .fetch_one(self.pool)
                .await?,
            club_aliases: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM club_aliases")
                .fetch_one(self.pool)
                .await?,
            athletes: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM athletes")
                .fetch_one(self.pool)
                .await?,
            disciplines: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM disciplines")
                .fetch_one(self.pool)
                .await?,
            results: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM results")
                .fetch_one(self.pool)
                .await?,
            parser_runs: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM parser_runs")
                .fetch_one(self.pool)
                .await?,
            parsed_result_rows: sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM parsed_result_rows",
            )
            .fetch_one(self.pool)
            .await?,
            manual_overrides: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM manual_overrides")
                .fetch_one(self.pool)
                .await?,
            organizations: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM organizations")
                .fetch_one(self.pool)
                .await?,
            organization_aliases: sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM organization_aliases",
            )
            .fetch_one(self.pool)
            .await?,
            teams: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM teams")
                .fetch_one(self.pool)
                .await?,
            team_members: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM team_members")
                .fetch_one(self.pool)
                .await?,
            team_result_members: sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM team_result_members",
            )
            .fetch_one(self.pool)
            .await?,
        })
    }
}

fn competition_organizer_code(competition: &NewCompetition) -> Option<&str> {
    match competition.scope.as_str() {
        "KM" => competition.association_code.as_deref().or(Some("OD")),
        "LM" => Some("NDSB"),
        "DM" => Some("DSB"),
        "WM" => Some("ISSF"),
        "Olympia" | "OLY" => Some("IOC"),
        _ => competition.organizer.as_deref(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        NewAthlete, NewClub, NewClubAlias, NewCompetition, NewDiscipline, NewImportRun,
        NewManualOverride, NewOrganization, NewOrganizationAlias, NewParsedResultRow, NewParserRun,
        NewResult, NewSourceDocument, NewTeam, NewTeamMember, NewTeamResultMember,
        StorageRepository,
    };
    use crate::storage::{Database, DatabaseConfig};

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn stores_canonical_entities_and_raw_result_values() {
        let path = temp_database_path();
        let database = Database::new(DatabaseConfig { path: path.clone() });
        let pool = database.migrated_pool().await.expect("database migrates");
        let repository = StorageRepository::new(&pool);

        let source_document_id = repository
            .upsert_source_document(&source_document())
            .await
            .expect("source document is stored");
        let import_run_id = repository
            .insert_import_run(&import_run(source_document_id))
            .await
            .expect("import run is stored");
        let parser_run_id = repository
            .upsert_parser_run(&parser_run(import_run_id))
            .await
            .expect("parser run is stored");
        let competition_id = repository
            .upsert_competition(&competition())
            .await
            .expect("competition is stored");
        let club_id = repository
            .upsert_club(&club())
            .await
            .expect("club is stored");
        repository
            .upsert_club_alias(&club_alias(club_id))
            .await
            .expect("club alias is stored");
        let aliases = repository
            .active_club_aliases()
            .await
            .expect("club aliases are loaded");
        let athlete_id = repository
            .upsert_athlete(&athlete())
            .await
            .expect("athlete is stored");
        let discipline_id = repository
            .upsert_discipline(&discipline())
            .await
            .expect("discipline is stored");
        repository
            .upsert_manual_override(&manual_override())
            .await
            .expect("manual override is stored");
        let organization_id = repository
            .upsert_organization(&organization())
            .await
            .expect("organization is stored");
        repository
            .upsert_organization_alias(&organization_alias(organization_id))
            .await
            .expect("organization alias is stored");
        let overrides = repository
            .active_manual_overrides("club", "canonical_name")
            .await
            .expect("manual overrides are loaded");
        let parsed_row_id = repository
            .insert_parsed_result_row_once(&parsed_result_row(parser_run_id, source_document_id))
            .await
            .expect("parsed row is stored")
            .id;

        let result_id = repository
            .insert_result(&result(
                import_run_id,
                source_document_id,
                parsed_row_id,
                competition_id,
                club_id,
                athlete_id,
                discipline_id,
            ))
            .await
            .expect("result is stored");
        store_team_fixture(
            &repository,
            source_document_id,
            parsed_row_id,
            result_id,
            competition_id,
            club_id,
            athlete_id,
            discipline_id,
        )
        .await;
        let duplicated_result_id = repository
            .insert_result(&result(
                import_run_id,
                source_document_id,
                parsed_row_id,
                competition_id,
                club_id,
                athlete_id,
                discipline_id,
            ))
            .await
            .expect("duplicate result resolves to existing id");
        let counts = repository.counts().await.expect("counts are readable");

        assert!(result_id > 0);
        assert_eq!(duplicated_result_id, result_id);
        assert_eq!(counts.source_documents, 1);
        assert_eq!(counts.import_runs, 1);
        assert_eq!(counts.competitions, 1);
        assert_eq!(counts.clubs, 1);
        assert_eq!(counts.club_aliases, 1);
        assert_eq!(counts.athletes, 1);
        assert_eq!(counts.disciplines, 1);
        assert_eq!(counts.results, 1);
        assert_eq!(counts.parser_runs, 1);
        assert_eq!(counts.parsed_result_rows, 1);
        assert_eq!(counts.manual_overrides, 1);
        assert!(counts.organizations >= 6);
        assert!(counts.organization_aliases >= 6);
        assert_eq!(counts.teams, 1);
        assert_eq!(counts.team_members, 1);
        assert_eq!(counts.team_result_members, 1);
        assert_eq!(overrides[0].old_value, "Schützenverein Reinfeld 1");
        assert_eq!(overrides[0].new_value, "Schützenverein Reinfeld");
        assert_eq!(aliases[0].alias, "SchV Reinfeld");
        assert_eq!(aliases[0].canonical_name, "Schützenverein Reinfeld");

        pool.close().await;
        remove_database_files(&path);
    }

    #[allow(clippy::too_many_arguments)]
    async fn store_team_fixture(
        repository: &StorageRepository<'_>,
        source_document_id: i64,
        parsed_row_id: i64,
        result_id: i64,
        competition_id: i64,
        club_id: i64,
        athlete_id: i64,
        discipline_id: i64,
    ) {
        let team_id = repository
            .upsert_team(&team(
                source_document_id,
                parsed_row_id,
                competition_id,
                club_id,
                discipline_id,
            ))
            .await
            .expect("team is stored");
        let team_member_id = repository
            .upsert_team_member(&team_member(team_id, athlete_id))
            .await
            .expect("team member is stored");
        repository
            .upsert_team_result_member(&team_result_member(
                team_id,
                result_id,
                athlete_id,
                team_member_id,
            ))
            .await
            .expect("team result member is stored");
    }

    fn temp_database_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pdf-explorer-repository-test-{}.sqlite",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is after unix epoch")
                .as_nanos()
        ))
    }

    fn source_document() -> NewSourceDocument {
        NewSourceDocument {
            source_name: "LM".to_owned(),
            url: "https://example.test/1.10.10.pdf".to_owned(),
            local_path: Some("data/archive/2026/lm/1.10.10.pdf".to_owned()),
            sha256: Some("abc123".to_owned()),
            etag: Some("\"etag\"".to_owned()),
            last_modified: None,
            file_size_bytes: Some(42),
            classification: "david21".to_owned(),
        }
    }

    fn import_run(source_document_id: i64) -> NewImportRun {
        NewImportRun {
            source_document_id: Some(source_document_id),
            source_name: "LM".to_owned(),
            run_kind: "parse".to_owned(),
            parser_name: Some("david21".to_owned()),
            parser_version: Some("test".to_owned()),
            input_path: Some("data/archive/2026/lm/1.10.10.pdf".to_owned()),
            input_hash: Some("abc123".to_owned()),
            status: "success".to_owned(),
            error: None,
        }
    }

    fn parser_run(import_run_id: i64) -> NewParserRun {
        NewParserRun {
            import_run_id: Some(import_run_id),
            source_name: "LM".to_owned(),
            source_kind: "podium-export".to_owned(),
            parser_name: "podium-export-json".to_owned(),
            parser_version: "test".to_owned(),
            input_path: "reports/podium-export.json".to_owned(),
            input_hash: "abc123".to_owned(),
            source_report_path: Some("reports/crawl-report.json".to_owned()),
            export_generated_at: Some("2026-09-03T00:00:00Z".to_owned()),
            status: "success".to_owned(),
            error: None,
        }
    }

    fn competition() -> NewCompetition {
        NewCompetition {
            code: "LM-2026".to_owned(),
            name: "Landesmeisterschaft 2026".to_owned(),
            year: 2026,
            scope: "LM".to_owned(),
            organizer: Some("NDSB".to_owned()),
            association_code: Some("NDSB".to_owned()),
            country_code: Some("DE".to_owned()),
            date_from: Some("2026-05-30".to_owned()),
            date_to: None,
        }
    }

    fn club() -> NewClub {
        NewClub {
            canonical_name: "Schützenverein Reinfeld".to_owned(),
            association_code: Some("OD".to_owned()),
            source: Some("parser".to_owned()),
        }
    }

    fn club_alias(club_id: i64) -> NewClubAlias {
        NewClubAlias {
            club_id,
            alias: "SchV Reinfeld".to_owned(),
            association_code: Some("OD".to_owned()),
            source: Some("parser".to_owned()),
            status: "active".to_owned(),
        }
    }

    fn athlete() -> NewAthlete {
        NewAthlete {
            canonical_name: "Soares dos Reis, Maximilian".to_owned(),
            sort_name: Some("Soares dos Reis, Maximilian".to_owned()),
        }
    }

    fn discipline() -> NewDiscipline {
        NewDiscipline {
            code: Some("1.80.40".to_owned()),
            name: "KK liegend".to_owned(),
        }
    }

    fn manual_override() -> NewManualOverride {
        NewManualOverride {
            scope: "global".to_owned(),
            entity_type: "club".to_owned(),
            entity_id: None,
            source_document_id: None,
            parsed_result_row_id: None,
            field_name: "canonical_name".to_owned(),
            old_value: "Schützenverein Reinfeld 1".to_owned(),
            new_value: "Schützenverein Reinfeld".to_owned(),
            reason: Some("Mannschaftsnummer am Vereinsnamen".to_owned()),
            status: "active".to_owned(),
        }
    }

    fn organization() -> NewOrganization {
        NewOrganization {
            code: "KM-OD".to_owned(),
            name: "Kreismeisterschaft Stormarn".to_owned(),
            organization_type: "competition_series".to_owned(),
            parent_id: None,
            country_code: Some("DE".to_owned()),
        }
    }

    fn organization_alias(organization_id: i64) -> NewOrganizationAlias {
        NewOrganizationAlias {
            organization_id,
            alias: "KM Stormarn".to_owned(),
            source: Some("test".to_owned()),
            status: "active".to_owned(),
        }
    }

    fn team(
        source_document_id: i64,
        parsed_result_row_id: i64,
        competition_id: i64,
        club_id: i64,
        discipline_id: i64,
    ) -> NewTeam {
        NewTeam {
            competition_id,
            club_id: Some(club_id),
            discipline_id: Some(discipline_id),
            source_document_id: Some(source_document_id),
            parsed_result_row_id: Some(parsed_result_row_id),
            canonical_name: "Schützenverein Reinfeld I".to_owned(),
            team_number: Some("I".to_owned()),
            raw_team_name: Some("Schützenverein Reinfeld 1".to_owned()),
            rank: Some(2),
            score: None,
            medal: Some("silver".to_owned()),
            event_class: Some("Herren IV".to_owned()),
            source_fingerprint: Some("abc123|team|LM|2026|1.80.40|2|Reinfeld-I".to_owned()),
            canonical_fingerprint: Some("LM|2026|1.80.40|team|2|Reinfeld-I".to_owned()),
            conflict_status: "none".to_owned(),
        }
    }

    fn team_member(team_id: i64, athlete_id: i64) -> NewTeamMember {
        NewTeamMember {
            team_id,
            athlete_id: Some(athlete_id),
            member_order: 0,
            display_name: "Soares dos Reis, Maximilian".to_owned(),
            raw_name: Some("Soares dos Reis, Maximilian".to_owned()),
        }
    }

    fn team_result_member(
        team_id: i64,
        result_id: i64,
        athlete_id: i64,
        team_member_id: i64,
    ) -> NewTeamResultMember {
        NewTeamResultMember {
            team_id,
            result_id,
            athlete_id: Some(athlete_id),
            team_member_id: Some(team_member_id),
            member_order: 0,
            score: Some(621.7),
            medal: Some("silver".to_owned()),
            raw_name: Some("Soares dos Reis, Maximilian".to_owned()),
        }
    }

    fn parsed_result_row(parser_run_id: i64, source_document_id: i64) -> NewParsedResultRow {
        NewParsedResultRow {
            parser_run_id,
            source_document_id: Some(source_document_id),
            row_index: 0,
            row_fingerprint: "abc123|0|Soares dos Reis, Maximilian".to_owned(),
            canonical_fingerprint: "LM|2026|1.80.40|individual|2|Soares dos Reis, Maximilian"
                .to_owned(),
            source_name: "LM".to_owned(),
            competition_year: 2026,
            competition_scope: "LM".to_owned(),
            result_kind: "individual".to_owned(),
            rank: Some(2),
            score: Some(621.7),
            raw_shooter_name: Some("Soares dos Reis, Maximilian".to_owned()),
            normalized_shooter_name: Some("Soares dos Reis, Maximilian".to_owned()),
            raw_club_name: Some("Schützenverein Reinfeld 1".to_owned()),
            normalized_club_name: Some("Schützenverein Reinfeld".to_owned()),
            association_code: Some("OD".to_owned()),
            raw_discipline: Some("1.80.40 KK liegend".to_owned()),
            normalized_discipline: Some("KK liegend".to_owned()),
            discipline_code: Some("1.80.40".to_owned()),
            class_name: Some("Herren IV".to_owned()),
            event_name: Some("Landesmeisterschaft 2026".to_owned()),
            event_date: Some("2026-05-30".to_owned()),
            pdf_url: Some("https://example.test/1.80.40.pdf".to_owned()),
            local_path: Some("data/archive/2026/lm/1.80.40.pdf".to_owned()),
            raw_payload: Some(r#"{"rank":2,"score":621.7}"#.to_owned()),
            conflict_status: "none".to_owned(),
            conflict_result_id: None,
        }
    }

    fn result(
        import_run_id: i64,
        source_document_id: i64,
        parsed_result_row_id: i64,
        competition_id: i64,
        club_id: i64,
        athlete_id: i64,
        discipline_id: i64,
    ) -> NewResult {
        NewResult {
            import_run_id: Some(import_run_id),
            source_document_id: Some(source_document_id),
            parsed_result_row_id: Some(parsed_result_row_id),
            competition_id,
            athlete_id: Some(athlete_id),
            club_id: Some(club_id),
            discipline_id: Some(discipline_id),
            result_kind: "individual".to_owned(),
            rank: Some(2),
            score: Some(621.7),
            medal: Some("silver".to_owned()),
            participation_only: false,
            event_class: Some("Herren IV".to_owned()),
            stage: "final".to_owned(),
            raw_shooter_name: Some("Soares dos Reis, Maximilian".to_owned()),
            raw_club_name: Some("Schützenverein Reinfeld 1".to_owned()),
            raw_discipline: Some("1.80.40 KK liegend".to_owned()),
            raw_payload: Some(r#"{"rank":2,"score":621.7}"#.to_owned()),
            source_fingerprint: Some(
                "abc123|LM|2026|1.80.40|2|Soares dos Reis, Maximilian".to_owned(),
            ),
            canonical_fingerprint: Some(
                "LM|2026|1.80.40|individual|2|Soares dos Reis, Maximilian".to_owned(),
            ),
            conflict_status: "none".to_owned(),
        }
    }

    fn remove_database_files(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    }
}
