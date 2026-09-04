use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::export::{ParticipationExport, ParticipationMatch};
use crate::storage::{
    Database, DatabaseConfig, NewAthlete, NewClub, NewClubAlias, NewCompetition, NewImportRun,
    NewParsedResultRow, NewParserRun, NewResult, NewSourceDocument, StorageRepository,
};

const IMPORT_KIND: &str = "participation-export-import";
const PARSER_NAME: &str = "participation-export-json";

#[derive(Debug, Clone)]
pub struct ParticipationImportConfig {
    pub input_path: PathBuf,
    pub database_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParticipationImportReport {
    pub input_path: PathBuf,
    pub database_path: PathBuf,
    pub input_hash: String,
    pub source_name: String,
    pub year: i64,
    pub competition_id: i64,
    pub import_run_id: i64,
    pub parser_run_id: i64,
    pub imported_parser_rows: usize,
    pub skipped_duplicate_parser_rows: usize,
    pub imported_results: usize,
    pub skipped_duplicate_results: usize,
}

pub struct ParticipationImporter {
    config: ParticipationImportConfig,
}

#[derive(Debug, Clone, Copy)]
struct ImportContext<'a> {
    import_run_id: i64,
    competition_id: i64,
    parser_run_id: i64,
    input_hash: &'a str,
    year: i64,
    manual_overrides: &'a ManualOverrideSet,
}

#[derive(Debug, Clone, Copy, Default)]
struct ImportedItems {
    imported_parser_rows: usize,
    skipped_duplicate_parser_rows: usize,
    imported_results: usize,
    skipped_duplicate_results: usize,
}

#[derive(Debug, Clone, Default)]
struct ManualOverrideSet {
    clubs: BTreeMap<String, String>,
    athletes: BTreeMap<String, String>,
}

impl ManualOverrideSet {
    async fn load(repository: &StorageRepository<'_>) -> Result<Self> {
        let clubs = repository
            .active_manual_overrides("club", "canonical_name")
            .await?
            .into_iter()
            .map(|manual_override| (manual_override.old_value, manual_override.new_value))
            .chain(
                repository
                    .active_club_aliases()
                    .await?
                    .into_iter()
                    .map(|alias| (alias.alias, alias.canonical_name)),
            )
            .collect();
        let athletes = repository
            .active_manual_overrides("athlete", "canonical_name")
            .await?
            .into_iter()
            .map(|manual_override| (manual_override.old_value, manual_override.new_value))
            .collect();
        Ok(Self { clubs, athletes })
    }

    fn club_name(&self, raw_club: &str, parser_club: &str) -> String {
        self.clubs
            .get(parser_club)
            .or_else(|| self.clubs.get(raw_club))
            .cloned()
            .unwrap_or_else(|| parser_club.to_owned())
    }

    fn athlete_name(&self, raw_athlete: &str) -> String {
        self.athletes
            .get(raw_athlete)
            .cloned()
            .unwrap_or_else(|| raw_athlete.to_owned())
    }
}

impl ParticipationImporter {
    #[must_use]
    pub const fn new(config: ParticipationImportConfig) -> Self {
        Self { config }
    }

    /// Imports a participation export into the local `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if the export cannot be read, parsed, migrated, or stored.
    pub async fn run(&self) -> Result<ParticipationImportReport> {
        let (export, input_hash) = read_participation_export(&self.config.input_path)?;
        let year = export_year(&export)?;
        let database = Database::new(DatabaseConfig {
            path: self.config.database_path.clone(),
        });
        let pool = database.migrated_pool().await?;
        let repository = StorageRepository::new(&pool);
        let competition_id = repository
            .upsert_competition(&competition_from_export(&export, year))
            .await?;
        let import_run_id = repository
            .insert_import_run(&import_run_from_export(
                &export,
                &self.config.input_path,
                &input_hash,
            ))
            .await?;
        let parser_run_id = repository
            .upsert_parser_run(&parser_run_from_export(
                &export,
                import_run_id,
                &self.config.input_path,
                &input_hash,
            ))
            .await?;
        let manual_overrides = ManualOverrideSet::load(&repository).await?;
        let imported = import_matches(
            &repository,
            &export.matches,
            ImportContext {
                import_run_id,
                competition_id,
                parser_run_id,
                input_hash: &input_hash,
                year,
                manual_overrides: &manual_overrides,
            },
        )
        .await?;
        pool.close().await;

        Ok(ParticipationImportReport {
            input_path: self.config.input_path.clone(),
            database_path: self.config.database_path.clone(),
            input_hash,
            source_name: export.results_source_name,
            year,
            competition_id,
            import_run_id,
            parser_run_id,
            imported_parser_rows: imported.imported_parser_rows,
            skipped_duplicate_parser_rows: imported.skipped_duplicate_parser_rows,
            imported_results: imported.imported_results,
            skipped_duplicate_results: imported.skipped_duplicate_results,
        })
    }
}

fn read_participation_export(path: &Path) -> Result<(ParticipationExport, String)> {
    let bytes = fs::read(path)
        .with_context(|| format!("could not read participation export {}", path.display()))?;
    let input_hash = sha256_hex(&bytes);
    let export: ParticipationExport =
        serde_json::from_slice(&bytes).context("could not parse participation export JSON")?;
    Ok((export, input_hash))
}

async fn import_matches(
    repository: &StorageRepository<'_>,
    matches: &[ParticipationMatch],
    context: ImportContext<'_>,
) -> Result<ImportedItems> {
    let mut imported = ImportedItems::default();
    let mut row_index = 0_i64;
    for item in matches {
        let shooters = participation_shooters(item);
        for shooter in shooters {
            import_participation(
                repository,
                item,
                shooter.as_deref(),
                row_index,
                context,
                &mut imported,
            )
            .await?;
            row_index += 1;
        }
    }
    Ok(imported)
}

fn participation_shooters(item: &ParticipationMatch) -> Vec<Option<String>> {
    if item.shooters.is_empty() {
        vec![None]
    } else {
        item.shooters.iter().cloned().map(Some).collect()
    }
}

async fn import_participation(
    repository: &StorageRepository<'_>,
    item: &ParticipationMatch,
    raw_shooter: Option<&str>,
    row_index: i64,
    context: ImportContext<'_>,
    imported: &mut ImportedItems,
) -> Result<()> {
    let source_document_id = repository
        .upsert_source_document(&source_document_from_match(item)?)
        .await?;
    let parser_club = canonical_match_club(item);
    let canonical_club = context.manual_overrides.club_name(&item.club, &parser_club);
    let canonical_athlete =
        raw_shooter.map(|shooter| context.manual_overrides.athlete_name(shooter));
    let raw_payload = raw_payload(item, raw_shooter)?;
    let canonical_fingerprint = canonical_result_fingerprint(
        item,
        context.year,
        &canonical_club,
        canonical_athlete.as_deref(),
    );
    let source_fingerprint = source_result_fingerprint(context.input_hash, &canonical_fingerprint);
    let parsed_row = parsed_result_row_from_match(
        item,
        raw_shooter,
        canonical_athlete.as_deref(),
        context,
        source_document_id,
        row_index,
        &canonical_club,
        &raw_payload,
        &canonical_fingerprint,
    );
    let stored_row = repository
        .insert_parsed_result_row_once(&parsed_row)
        .await?;
    if stored_row.inserted {
        imported.imported_parser_rows += 1;
    } else {
        imported.skipped_duplicate_parser_rows += 1;
    }

    let club_id = repository
        .upsert_club(&club_from_match(item, &canonical_club))
        .await?;
    store_club_aliases(repository, club_id, item, &canonical_club).await?;
    let athlete_id = if let Some(canonical_athlete) = canonical_athlete.as_deref() {
        Some(
            repository
                .upsert_athlete(&athlete_from_name(canonical_athlete))
                .await?,
        )
    } else {
        None
    };
    let result = result_from_match(
        item,
        raw_shooter,
        context,
        source_document_id,
        stored_row.id,
        club_id,
        athlete_id,
        &raw_payload,
        &source_fingerprint,
        &canonical_fingerprint,
    );
    let stored = repository.insert_result_once(&result).await?;
    if stored.inserted {
        imported.imported_results += 1;
    } else {
        imported.skipped_duplicate_results += 1;
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn export_year(export: &ParticipationExport) -> Result<i64> {
    export
        .matches
        .iter()
        .find_map(|item| year_from_path(&item.local_path))
        .or_else(|| year_from_path(&export.results_report_path))
        .with_context(|| {
            format!(
                "could not infer competition year from {}",
                export.results_report_path.display()
            )
        })
}

fn year_from_path(path: &Path) -> Option<i64> {
    path.iter()
        .filter_map(|part| part.to_str())
        .find_map(first_four_digit_year)
}

fn first_four_digit_year(value: &str) -> Option<i64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| part.len() == 4)
        .and_then(|part| part.parse().ok())
}

fn competition_from_export(export: &ParticipationExport, year: i64) -> NewCompetition {
    NewCompetition {
        code: format!("DM-{year}"),
        name: format!("{} {year}", export.results_source_name),
        year,
        scope: "DM".to_owned(),
        organizer: Some("DSB".to_owned()),
        association_code: None,
        country_code: Some("DE".to_owned()),
        date_from: None,
        date_to: None,
    }
}

fn import_run_from_export(
    export: &ParticipationExport,
    input_path: &Path,
    input_hash: &str,
) -> NewImportRun {
    NewImportRun {
        source_document_id: None,
        source_name: export.results_source_name.clone(),
        run_kind: IMPORT_KIND.to_owned(),
        parser_name: Some(PARSER_NAME.to_owned()),
        parser_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        input_path: Some(input_path.display().to_string()),
        input_hash: Some(input_hash.to_owned()),
        status: "success".to_owned(),
        error: None,
    }
}

fn parser_run_from_export(
    export: &ParticipationExport,
    import_run_id: i64,
    input_path: &Path,
    input_hash: &str,
) -> NewParserRun {
    NewParserRun {
        import_run_id: Some(import_run_id),
        source_name: export.results_source_name.clone(),
        source_kind: "participation-export".to_owned(),
        parser_name: PARSER_NAME.to_owned(),
        parser_version: env!("CARGO_PKG_VERSION").to_owned(),
        input_path: input_path.display().to_string(),
        input_hash: input_hash.to_owned(),
        source_report_path: Some(export.results_report_path.display().to_string()),
        export_generated_at: Some(export.generated_at.to_rfc3339()),
        status: "success".to_owned(),
        error: None,
    }
}

fn source_document_from_match(item: &ParticipationMatch) -> Result<NewSourceDocument> {
    Ok(NewSourceDocument {
        source_name: item.source_name.clone(),
        url: item.pdf_url.clone(),
        local_path: Some(item.local_path.display().to_string()),
        sha256: local_file_hash(&item.local_path)?,
        etag: None,
        last_modified: None,
        file_size_bytes: local_file_size(&item.local_path)?,
        classification: "participation".to_owned(),
    })
}

fn local_file_hash(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).with_context(|| format!("could not read PDF {}", path.display()))?;
    Ok(Some(sha256_hex(&bytes)))
}

fn local_file_size(path: &Path) -> Result<Option<i64>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata =
        fs::metadata(path).with_context(|| format!("could not stat PDF {}", path.display()))?;
    Ok(Some(
        i64::try_from(metadata.len()).context("PDF file size does not fit into i64")?,
    ))
}

fn club_from_match(item: &ParticipationMatch, canonical_name: &str) -> NewClub {
    NewClub {
        canonical_name: canonical_name.to_owned(),
        association_code: Some("OD".to_owned()),
        source: Some(item.source_name.clone()),
    }
}

async fn store_club_aliases(
    repository: &StorageRepository<'_>,
    club_id: i64,
    item: &ParticipationMatch,
    canonical_club: &str,
) -> Result<()> {
    for alias in [item.club.as_str(), canonical_club] {
        let alias = alias.trim();
        if !alias.is_empty() {
            repository
                .upsert_club_alias(&NewClubAlias {
                    club_id,
                    alias: alias.to_owned(),
                    association_code: None,
                    source: Some("participation-export".to_owned()),
                    status: "active".to_owned(),
                })
                .await?;
        }
    }
    Ok(())
}

fn athlete_from_name(canonical_name: &str) -> NewAthlete {
    NewAthlete {
        canonical_name: canonical_name.to_owned(),
        sort_name: Some(canonical_name.to_owned()),
    }
}

fn raw_payload(item: &ParticipationMatch, raw_shooter: Option<&str>) -> Result<String> {
    serde_json::to_string(&serde_json::json!({
        "club": item.club,
        "canonical_club": item.canonical_club,
        "shooter": raw_shooter,
        "source_name": item.source_name,
        "pdf_url": item.pdf_url,
        "local_path": item.local_path,
        "text_char_count": item.text_char_count,
    }))
    .context("could not serialize participation raw payload")
}

#[allow(clippy::too_many_arguments)]
fn parsed_result_row_from_match(
    item: &ParticipationMatch,
    raw_shooter: Option<&str>,
    canonical_athlete: Option<&str>,
    context: ImportContext<'_>,
    source_document_id: i64,
    row_index: i64,
    canonical_club: &str,
    raw_payload: &str,
    canonical_fingerprint: &str,
) -> NewParsedResultRow {
    NewParsedResultRow {
        parser_run_id: context.parser_run_id,
        source_document_id: Some(source_document_id),
        row_index,
        row_fingerprint: parsed_row_fingerprint(context.input_hash, row_index),
        canonical_fingerprint: canonical_fingerprint.to_owned(),
        source_name: item.source_name.clone(),
        competition_year: context.year,
        competition_scope: "DM".to_owned(),
        result_kind: "participation".to_owned(),
        rank: None,
        score: None,
        raw_shooter_name: raw_shooter.map(str::to_owned),
        normalized_shooter_name: canonical_athlete.map(str::to_owned),
        raw_club_name: Some(item.club.clone()),
        normalized_club_name: Some(canonical_club.to_owned()),
        association_code: Some("OD".to_owned()),
        raw_discipline: None,
        normalized_discipline: None,
        discipline_code: None,
        class_name: None,
        event_name: Some("Deutsche Meisterschaft".to_owned()),
        event_date: None,
        pdf_url: Some(item.pdf_url.clone()),
        local_path: Some(item.local_path.display().to_string()),
        raw_payload: Some(raw_payload.to_owned()),
        conflict_status: "none".to_owned(),
        conflict_result_id: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn result_from_match(
    item: &ParticipationMatch,
    raw_shooter: Option<&str>,
    context: ImportContext<'_>,
    source_document_id: i64,
    parsed_result_row_id: i64,
    club_id: i64,
    athlete_id: Option<i64>,
    raw_payload: &str,
    source_fingerprint: &str,
    canonical_fingerprint: &str,
) -> NewResult {
    NewResult {
        import_run_id: Some(context.import_run_id),
        source_document_id: Some(source_document_id),
        parsed_result_row_id: Some(parsed_result_row_id),
        competition_id: context.competition_id,
        athlete_id,
        club_id: Some(club_id),
        discipline_id: None,
        result_kind: "participation".to_owned(),
        rank: None,
        score: None,
        medal: None,
        participation_only: true,
        event_class: None,
        stage: "participation".to_owned(),
        raw_shooter_name: raw_shooter.map(str::to_owned),
        raw_club_name: Some(item.club.clone()),
        raw_discipline: None,
        raw_payload: Some(raw_payload.to_owned()),
        source_fingerprint: Some(source_fingerprint.to_owned()),
        canonical_fingerprint: Some(canonical_fingerprint.to_owned()),
        conflict_status: "none".to_owned(),
    }
}

fn parsed_row_fingerprint(input_hash: &str, row_index: i64) -> String {
    format!("{input_hash}|participation-row|{row_index}")
}

fn source_result_fingerprint(input_hash: &str, canonical_fingerprint: &str) -> String {
    format!("{input_hash}|{canonical_fingerprint}")
}

fn canonical_result_fingerprint(
    item: &ParticipationMatch,
    year: i64,
    canonical_club: &str,
    canonical_athlete: Option<&str>,
) -> String {
    [
        &item.source_name,
        &year.to_string(),
        "participation",
        canonical_club,
        canonical_athlete.unwrap_or("club-only"),
        &item.pdf_url,
    ]
    .join("|")
}

fn canonical_match_club(item: &ParticipationMatch) -> String {
    if item.canonical_club.trim().is_empty() {
        item.club.clone()
    } else {
        item.canonical_club.clone()
    }
}

#[allow(dead_code)]
fn parse_german_year(value: &str) -> Option<i64> {
    NaiveDate::parse_from_str(value, "%d.%m.%Y")
        .ok()
        .map(|date| i64::from(date.year()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::Utc;

    use super::{ParticipationImportConfig, ParticipationImporter, first_four_digit_year};
    use crate::export::{ParticipationExport, ParticipationMatch};
    use crate::storage::{Database, DatabaseConfig};

    #[test]
    fn extracts_year_from_path() {
        assert_eq!(first_four_digit_year("ndsb-2026-dm"), Some(2026));
    }

    #[tokio::test]
    async fn imports_participation_export_as_dm_participation_results() {
        let dir = std::env::temp_dir().join(format!(
            "pdf-explorer-participation-import-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is after unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir is created");
        let input_path = dir.join("participation-export.json");
        let database_path = dir.join("participation-import.sqlite");
        fs::write(
            &input_path,
            serde_json::to_vec(&export()).expect("export serializes"),
        )
        .expect("export is written");

        let importer = ParticipationImporter::new(ParticipationImportConfig {
            input_path: input_path.clone(),
            database_path: database_path.clone(),
        });
        let first_report = importer.run().await.expect("first import succeeds");
        let second_report = importer.run().await.expect("second import succeeds");

        assert_eq!(first_report.imported_results, 2);
        assert_eq!(first_report.imported_parser_rows, 2);
        assert_eq!(second_report.imported_results, 0);
        assert_eq!(second_report.imported_parser_rows, 0);

        let database = Database::new(DatabaseConfig {
            path: database_path.clone(),
        });
        let pool = database.migrated_pool().await.expect("database opens");
        let competition = sqlx::query_as::<_, (String, i64)>(
            "SELECT scope, year FROM competitions WHERE code = 'DM-2026'",
        )
        .fetch_one(&pool)
        .await
        .expect("competition exists");
        let result_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM results WHERE participation_only = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("result count is readable");
        let athlete_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM athletes")
            .fetch_one(&pool)
            .await
            .expect("athlete count is readable");
        pool.close().await;

        assert_eq!(competition, ("DM".to_owned(), 2026));
        assert_eq!(result_count, 2);
        assert_eq!(athlete_count, 2);

        let _ = fs::remove_dir_all(dir);
    }

    fn export() -> ParticipationExport {
        ParticipationExport {
            generated_at: Utc::now(),
            club_source_report_path: "reports/archive/2026/landesmeisterschaften/crawl-report.json"
                .into(),
            results_report_path: "reports/archive/2026/deutsche-meisterschaften/crawl-report.json"
                .into(),
            club_source_name: "landesmeisterschaften".to_owned(),
            results_source_name: "deutsche-meisterschaften".to_owned(),
            focus_association_code: "OD".to_owned(),
            known_club_count: 1,
            matched_club_count: 1,
            match_count: 1,
            known_clubs: vec!["Ahrensburger Schützengilde".to_owned()],
            matches: vec![ParticipationMatch {
                club: "Ahrensburger Schützengilde".to_owned(),
                canonical_club: "Ahrensburger Schützengilde".to_owned(),
                shooters: vec!["Test, Tina".to_owned(), "Beispiel, Bert".to_owned()],
                source_name: "deutsche-meisterschaften".to_owned(),
                pdf_url: "https://example.test/dm.pdf".to_owned(),
                local_path: "data/archive/2026/deutsche-meisterschaften/downloads/dm.pdf".into(),
                text_char_count: 120,
            }],
        }
    }
}
