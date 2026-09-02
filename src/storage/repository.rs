use anyhow::Result;
use sqlx::SqlitePool;

use super::models::{
    NewAthlete, NewClub, NewCompetition, NewDiscipline, NewImportRun, NewResult, NewSourceDocument,
    StorageCounts,
};

pub struct StorageRepository<'a> {
    pool: &'a SqlitePool,
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

    /// Stores a canonical result and returns its technical ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the database write fails.
    pub async fn insert_result(&self, result: &NewResult) -> Result<i64> {
        let participation_only = i64::from(result.participation_only);
        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO results (
                import_run_id, source_document_id, competition_id, athlete_id,
                club_id, discipline_id, result_kind, rank, score, medal,
                participation_only, event_class, stage, raw_shooter_name,
                raw_club_name, raw_discipline, raw_payload
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            athletes: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM athletes")
                .fetch_one(self.pool)
                .await?,
            disciplines: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM disciplines")
                .fetch_one(self.pool)
                .await?,
            results: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM results")
                .fetch_one(self.pool)
                .await?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        NewAthlete, NewClub, NewCompetition, NewDiscipline, NewImportRun, NewResult,
        NewSourceDocument, StorageRepository,
    };
    use crate::storage::{Database, DatabaseConfig};

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
        let competition_id = repository
            .upsert_competition(&competition())
            .await
            .expect("competition is stored");
        let club_id = repository
            .upsert_club(&club())
            .await
            .expect("club is stored");
        let athlete_id = repository
            .upsert_athlete(&athlete())
            .await
            .expect("athlete is stored");
        let discipline_id = repository
            .upsert_discipline(&discipline())
            .await
            .expect("discipline is stored");

        let result_id = repository
            .insert_result(&result(
                import_run_id,
                source_document_id,
                competition_id,
                club_id,
                athlete_id,
                discipline_id,
            ))
            .await
            .expect("result is stored");
        let counts = repository.counts().await.expect("counts are readable");

        assert!(result_id > 0);
        assert_eq!(counts.source_documents, 1);
        assert_eq!(counts.import_runs, 1);
        assert_eq!(counts.competitions, 1);
        assert_eq!(counts.clubs, 1);
        assert_eq!(counts.athletes, 1);
        assert_eq!(counts.disciplines, 1);
        assert_eq!(counts.results, 1);

        pool.close().await;
        remove_database_files(&path);
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

    fn result(
        import_run_id: i64,
        source_document_id: i64,
        competition_id: i64,
        club_id: i64,
        athlete_id: i64,
        discipline_id: i64,
    ) -> NewResult {
        NewResult {
            import_run_id: Some(import_run_id),
            source_document_id: Some(source_document_id),
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
        }
    }

    fn remove_database_files(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    }
}
