use anyhow::{Context, Result};

#[derive(Debug, Clone, Default)]
pub struct ResultFilters {
    pub search: Option<String>,
    pub year: Option<i64>,
    pub scope: Option<String>,
    pub association_code: Option<String>,
    pub result_kind: Option<String>,
    pub import_run_id: Option<i64>,
    pub athlete_id: Option<i64>,
    pub club_id: Option<i64>,
    pub source_document_id: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct AthleteFilters {
    pub search: Option<String>,
    pub club: Option<String>,
    pub year: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ClubFilters {
    pub search: Option<String>,
    pub association_code: Option<String>,
    pub year: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ImportRunRow {
    pub id: i64,
    pub source_name: String,
    pub run_kind: String,
    pub parser_name: Option<String>,
    pub parser_version: Option<String>,
    pub input_path: Option<String>,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub result_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ResultRow {
    pub id: i64,
    pub athlete_id: Option<i64>,
    pub athlete_name: Option<String>,
    pub club_id: Option<i64>,
    pub club_name: Option<String>,
    pub discipline: Option<String>,
    pub competition_name: String,
    pub competition_scope: String,
    pub competition_year: i64,
    pub result_kind: String,
    pub rank: Option<i64>,
    pub score: Option<f64>,
    pub medal: Option<String>,
    pub participation_only: i64,
    pub event_class: Option<String>,
    pub source_document_id: Option<i64>,
    pub source_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AthleteRow {
    pub id: i64,
    pub canonical_name: String,
    pub result_count: i64,
    pub club_count: i64,
    pub latest_year: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ClubRow {
    pub id: i64,
    pub canonical_name: String,
    pub association_code: Option<String>,
    pub result_count: i64,
    pub athlete_count: i64,
    pub latest_year: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ManualOverrideRow {
    pub id: i64,
    pub scope: String,
    pub entity_type: String,
    pub field_name: String,
    pub old_value: String,
    pub new_value: String,
    pub reason: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ParsedIssueRow {
    pub id: i64,
    pub source_name: String,
    pub competition_year: i64,
    pub competition_scope: String,
    pub conflict_status: String,
    pub raw_shooter_name: Option<String>,
    pub normalized_shooter_name: Option<String>,
    pub raw_club_name: Option<String>,
    pub normalized_club_name: Option<String>,
    pub raw_discipline: Option<String>,
    pub discipline_code: Option<String>,
    pub class_name: Option<String>,
    pub event_name: Option<String>,
    pub pdf_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SourceDocumentRow {
    pub id: i64,
    pub source_name: String,
    pub url: String,
    pub local_path: Option<String>,
    pub sha256: Option<String>,
    pub classification: String,
    pub result_count: i64,
    pub parser_row_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ParserRunRow {
    pub id: i64,
    pub source_name: String,
    pub source_kind: String,
    pub parser_name: String,
    pub parser_version: String,
    pub input_path: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub parsed_row_count: i64,
    pub issue_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CombinedEvaluationRow {
    pub athlete_id: Option<i64>,
    pub athlete_name: Option<String>,
    pub club_id: Option<i64>,
    pub club_name: Option<String>,
    pub year: i64,
    pub lm_rank: Option<i64>,
    pub lm_medal: Option<String>,
    pub lm_result_kind: String,
    pub lm_discipline: Option<String>,
    pub lm_event_class: Option<String>,
    pub has_dm_participation: i64,
}

pub struct ApplicationService<'a> {
    pool: &'a sqlx::SqlitePool,
}

impl<'a> ApplicationService<'a> {
    pub const fn new(pool: &'a sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn import_runs(&self) -> Result<Vec<ImportRunRow>> {
        sqlx::query_as::<_, ImportRunRow>(
            r"
            SELECT
                import_runs.id,
                import_runs.source_name,
                import_runs.run_kind,
                import_runs.parser_name,
                import_runs.parser_version,
                import_runs.input_path,
                import_runs.status,
                import_runs.started_at,
                import_runs.finished_at,
                COUNT(results.id) AS result_count
            FROM import_runs
            LEFT JOIN results ON results.import_run_id = import_runs.id
            GROUP BY import_runs.id
            ORDER BY import_runs.started_at DESC, import_runs.id DESC
            ",
        )
        .fetch_all(self.pool)
        .await
        .context("could not load import runs")
    }

    pub async fn results(&self, filters: &ResultFilters) -> Result<Vec<ResultRow>> {
        let search = like_filter(filters.search.as_deref());
        sqlx::query_as::<_, ResultRow>(
            r"
            SELECT
                results.id,
                results.athlete_id,
                athletes.canonical_name AS athlete_name,
                results.club_id,
                clubs.canonical_name AS club_name,
                TRIM(COALESCE(NULLIF(disciplines.code, ''), '') || ' ' || COALESCE(disciplines.name, '')) AS discipline,
                competitions.name AS competition_name,
                competitions.scope AS competition_scope,
                competitions.year AS competition_year,
                results.result_kind,
                results.rank,
                results.score,
                results.medal,
                results.participation_only,
                results.event_class,
                results.source_document_id,
                source_documents.url AS source_url
            FROM results
            JOIN competitions ON competitions.id = results.competition_id
            LEFT JOIN athletes ON athletes.id = results.athlete_id
            LEFT JOIN clubs ON clubs.id = results.club_id
            LEFT JOIN disciplines ON disciplines.id = results.discipline_id
            LEFT JOIN source_documents ON source_documents.id = results.source_document_id
            WHERE (? IS NULL OR results.import_run_id = ?)
                AND (? IS NULL OR results.athlete_id = ?)
                AND (? IS NULL OR results.club_id = ?)
                AND (? IS NULL OR results.source_document_id = ?)
                AND (? IS NULL OR competitions.year = ?)
                AND (? IS NULL OR competitions.scope = ?)
                AND (? IS NULL OR COALESCE(clubs.association_code, '') = ?)
                AND (? IS NULL OR results.result_kind = ?)
                AND (
                    ? IS NULL
                    OR COALESCE(athletes.canonical_name, '') LIKE ?
                    OR COALESCE(clubs.canonical_name, '') LIKE ?
                    OR COALESCE(disciplines.name, '') LIKE ?
                    OR COALESCE(disciplines.code, '') LIKE ?
                    OR COALESCE(results.event_class, '') LIKE ?
                    OR COALESCE(competitions.name, '') LIKE ?
                )
            ORDER BY competitions.year DESC, competitions.scope, results.rank, athlete_name
            LIMIT 5000
            ",
        )
        .bind(filters.import_run_id)
        .bind(filters.import_run_id)
        .bind(filters.athlete_id)
        .bind(filters.athlete_id)
        .bind(filters.club_id)
        .bind(filters.club_id)
        .bind(filters.source_document_id)
        .bind(filters.source_document_id)
        .bind(filters.year)
        .bind(filters.year)
        .bind(&filters.scope)
        .bind(&filters.scope)
        .bind(&filters.association_code)
        .bind(&filters.association_code)
        .bind(&filters.result_kind)
        .bind(&filters.result_kind)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .fetch_all(self.pool)
        .await
        .context("could not load results")
    }

    pub async fn athletes(&self, filters: &AthleteFilters) -> Result<Vec<AthleteRow>> {
        let search = like_filter(filters.search.as_deref());
        let club = like_filter(filters.club.as_deref());
        sqlx::query_as::<_, AthleteRow>(
            r"
            SELECT
                athletes.id,
                athletes.canonical_name,
                COUNT(results.id) AS result_count,
                COUNT(DISTINCT results.club_id) AS club_count,
                MAX(competitions.year) AS latest_year
            FROM athletes
            LEFT JOIN results ON results.athlete_id = athletes.id
            LEFT JOIN clubs ON clubs.id = results.club_id
            LEFT JOIN competitions ON competitions.id = results.competition_id
            WHERE (? IS NULL OR athletes.canonical_name LIKE ?)
                AND (? IS NULL OR clubs.canonical_name LIKE ?)
                AND (? IS NULL OR competitions.year = ?)
            GROUP BY athletes.id
            ORDER BY athletes.canonical_name
            LIMIT 5000
            ",
        )
        .bind(&search)
        .bind(&search)
        .bind(&club)
        .bind(&club)
        .bind(filters.year)
        .bind(filters.year)
        .fetch_all(self.pool)
        .await
        .context("could not load athletes")
    }

    pub async fn clubs(&self, filters: &ClubFilters) -> Result<Vec<ClubRow>> {
        let search = like_filter(filters.search.as_deref());
        sqlx::query_as::<_, ClubRow>(
            r"
            SELECT
                clubs.id,
                clubs.canonical_name,
                clubs.association_code,
                COUNT(results.id) AS result_count,
                COUNT(DISTINCT results.athlete_id) AS athlete_count,
                MAX(competitions.year) AS latest_year
            FROM clubs
            LEFT JOIN results ON results.club_id = clubs.id
            LEFT JOIN competitions ON competitions.id = results.competition_id
            WHERE (? IS NULL OR clubs.canonical_name LIKE ?)
                AND (? IS NULL OR COALESCE(clubs.association_code, '') = ?)
                AND (? IS NULL OR competitions.year = ?)
            GROUP BY clubs.id
            ORDER BY clubs.canonical_name
            LIMIT 5000
            ",
        )
        .bind(&search)
        .bind(&search)
        .bind(&filters.association_code)
        .bind(&filters.association_code)
        .bind(filters.year)
        .bind(filters.year)
        .fetch_all(self.pool)
        .await
        .context("could not load clubs")
    }

    pub async fn manual_overrides(&self) -> Result<Vec<ManualOverrideRow>> {
        sqlx::query_as::<_, ManualOverrideRow>(
            r"
            SELECT
                id, scope, entity_type, field_name, old_value, new_value,
                reason, status, created_at, updated_at
            FROM manual_overrides
            ORDER BY updated_at DESC, id DESC
            ",
        )
        .fetch_all(self.pool)
        .await
        .context("could not load manual overrides")
    }

    pub async fn parser_issues(&self) -> Result<Vec<ParsedIssueRow>> {
        sqlx::query_as::<_, ParsedIssueRow>(
            r"
            SELECT
                id, source_name, competition_year, competition_scope, conflict_status,
                raw_shooter_name, normalized_shooter_name, raw_club_name,
                normalized_club_name, raw_discipline, discipline_code, class_name,
                event_name, pdf_url
            FROM parsed_result_rows
            WHERE conflict_status <> 'none'
                OR normalized_shooter_name IS NULL
                OR normalized_club_name IS NULL
                OR normalized_discipline IS NULL
            ORDER BY competition_year DESC, source_name, id DESC
            ",
        )
        .fetch_all(self.pool)
        .await
        .context("could not load parser issues")
    }

    pub async fn source_documents(&self) -> Result<Vec<SourceDocumentRow>> {
        sqlx::query_as::<_, SourceDocumentRow>(
            r"
            SELECT
                source_documents.id,
                source_documents.source_name,
                source_documents.url,
                source_documents.local_path,
                source_documents.sha256,
                source_documents.classification,
                COUNT(DISTINCT results.id) AS result_count,
                COUNT(DISTINCT parsed_result_rows.id) AS parser_row_count
            FROM source_documents
            LEFT JOIN results ON results.source_document_id = source_documents.id
            LEFT JOIN parsed_result_rows
                ON parsed_result_rows.source_document_id = source_documents.id
            GROUP BY source_documents.id
            ORDER BY source_documents.source_name, source_documents.id DESC
            ",
        )
        .fetch_all(self.pool)
        .await
        .context("could not load source documents")
    }

    pub async fn source_document(&self, id: i64) -> Result<Option<SourceDocumentRow>> {
        sqlx::query_as::<_, SourceDocumentRow>(
            r"
            SELECT
                source_documents.id,
                source_documents.source_name,
                source_documents.url,
                source_documents.local_path,
                source_documents.sha256,
                source_documents.classification,
                COUNT(DISTINCT results.id) AS result_count,
                COUNT(DISTINCT parsed_result_rows.id) AS parser_row_count
            FROM source_documents
            LEFT JOIN results ON results.source_document_id = source_documents.id
            LEFT JOIN parsed_result_rows
                ON parsed_result_rows.source_document_id = source_documents.id
            WHERE source_documents.id = ?
            GROUP BY source_documents.id
            ",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await
        .context("could not load source document")
    }

    pub async fn parser_runs(&self) -> Result<Vec<ParserRunRow>> {
        sqlx::query_as::<_, ParserRunRow>(
            r"
            SELECT
                parser_runs.id,
                parser_runs.source_name,
                parser_runs.source_kind,
                parser_runs.parser_name,
                parser_runs.parser_version,
                parser_runs.input_path,
                parser_runs.status,
                parser_runs.started_at,
                parser_runs.finished_at,
                COUNT(parsed_result_rows.id) AS parsed_row_count,
                SUM(CASE
                    WHEN parsed_result_rows.conflict_status <> 'none'
                        OR parsed_result_rows.normalized_shooter_name IS NULL
                        OR parsed_result_rows.normalized_club_name IS NULL
                        OR parsed_result_rows.normalized_discipline IS NULL
                    THEN 1
                    ELSE 0
                END) AS issue_count
            FROM parser_runs
            LEFT JOIN parsed_result_rows ON parsed_result_rows.parser_run_id = parser_runs.id
            GROUP BY parser_runs.id
            ORDER BY parser_runs.started_at DESC, parser_runs.id DESC
            ",
        )
        .fetch_all(self.pool)
        .await
        .context("could not load parser runs")
    }

    pub async fn parser_run(&self, id: i64) -> Result<Option<ParserRunRow>> {
        sqlx::query_as::<_, ParserRunRow>(
            r"
            SELECT
                parser_runs.id,
                parser_runs.source_name,
                parser_runs.source_kind,
                parser_runs.parser_name,
                parser_runs.parser_version,
                parser_runs.input_path,
                parser_runs.status,
                parser_runs.started_at,
                parser_runs.finished_at,
                COUNT(parsed_result_rows.id) AS parsed_row_count,
                SUM(CASE
                    WHEN parsed_result_rows.conflict_status <> 'none'
                        OR parsed_result_rows.normalized_shooter_name IS NULL
                        OR parsed_result_rows.normalized_club_name IS NULL
                        OR parsed_result_rows.normalized_discipline IS NULL
                    THEN 1
                    ELSE 0
                END) AS issue_count
            FROM parser_runs
            LEFT JOIN parsed_result_rows ON parsed_result_rows.parser_run_id = parser_runs.id
            WHERE parser_runs.id = ?
            GROUP BY parser_runs.id
            ",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await
        .context("could not load parser run")
    }

    pub async fn combined_evaluation(
        &self,
        filters: &ResultFilters,
    ) -> Result<Vec<CombinedEvaluationRow>> {
        let search = like_filter(filters.search.as_deref());
        sqlx::query_as::<_, CombinedEvaluationRow>(
            r"
            SELECT
                athlete_id,
                athlete_name,
                club_id,
                club_name,
                year,
                lm_rank,
                lm_medal,
                lm_result_kind,
                lm_discipline,
                lm_event_class,
                has_dm_participation
            FROM lm_medals_with_dm_participation
            WHERE (? IS NULL OR year = ?)
                AND (? IS NULL OR COALESCE(club_name, '') LIKE ?)
                AND (
                    ? IS NULL
                    OR COALESCE(athlete_name, '') LIKE ?
                    OR COALESCE(club_name, '') LIKE ?
                    OR COALESCE(lm_discipline, '') LIKE ?
                    OR COALESCE(lm_event_class, '') LIKE ?
                )
            ORDER BY year DESC, club_name, athlete_name, lm_rank
            LIMIT 5000
            ",
        )
        .bind(filters.year)
        .bind(filters.year)
        .bind(&filters.association_code)
        .bind(
            filters
                .association_code
                .as_ref()
                .map(|value| format!("%{value}%")),
        )
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .fetch_all(self.pool)
        .await
        .context("could not load combined evaluation")
    }

    pub async fn club_names(&self) -> Result<Vec<String>> {
        self.names("SELECT canonical_name FROM clubs ORDER BY canonical_name")
            .await
    }

    pub async fn athlete_names(&self) -> Result<Vec<String>> {
        self.names("SELECT canonical_name FROM athletes ORDER BY canonical_name")
            .await
    }

    async fn names(&self, sql: &'static str) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(self.pool)
            .await
            .context("could not load names")
    }
}

fn like_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"))
}
