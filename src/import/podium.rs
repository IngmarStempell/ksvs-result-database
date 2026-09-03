use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::export::{PodiumExport, PodiumExportItem, PodiumResultKind};
use crate::storage::{
    CanonicalResultReference, Database, DatabaseConfig, NewAthlete, NewClub, NewCompetition,
    NewDiscipline, NewImportRun, NewParsedResultRow, NewParserRun, NewResult, NewSourceDocument,
    StorageRepository,
};

const IMPORT_KIND: &str = "podium-export-import";
const PARSER_NAME: &str = "podium-export-json";

#[derive(Debug, Clone)]
pub struct PodiumImportConfig {
    pub input_path: PathBuf,
    pub database_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodiumImportReport {
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
    pub conflict_count: usize,
}

pub struct PodiumImporter {
    config: PodiumImportConfig,
}

#[derive(Debug, Clone, Copy)]
struct ImportContext<'a> {
    import_run_id: i64,
    competition_id: i64,
    parser_run_id: i64,
    input_hash: &'a str,
    year: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct ImportedItems {
    imported_parser_rows: usize,
    skipped_duplicate_parser_rows: usize,
    imported_results: usize,
    skipped_duplicate_results: usize,
    conflict_count: usize,
}

impl PodiumImporter {
    #[must_use]
    pub const fn new(config: PodiumImportConfig) -> Self {
        Self { config }
    }

    /// Imports a `podium-export.json` file into the local `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if the export cannot be read, parsed, migrated, or stored.
    pub async fn run(&self) -> Result<PodiumImportReport> {
        let (export, input_hash) = read_podium_export(&self.config.input_path)?;
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

        let imported = import_items(
            &repository,
            &export.items,
            ImportContext {
                import_run_id,
                competition_id,
                parser_run_id,
                input_hash: &input_hash,
                year,
            },
        )
        .await?;

        pool.close().await;

        Ok(PodiumImportReport {
            input_path: self.config.input_path.clone(),
            database_path: self.config.database_path.clone(),
            input_hash,
            source_name: export.source_name,
            year,
            competition_id,
            import_run_id,
            parser_run_id,
            imported_parser_rows: imported.imported_parser_rows,
            skipped_duplicate_parser_rows: imported.skipped_duplicate_parser_rows,
            imported_results: imported.imported_results,
            skipped_duplicate_results: imported.skipped_duplicate_results,
            conflict_count: imported.conflict_count,
        })
    }
}

fn read_podium_export(path: &Path) -> Result<(PodiumExport, String)> {
    let bytes = fs::read(path)
        .with_context(|| format!("could not read podium export {}", path.display()))?;
    let input_hash = sha256_hex(&bytes);
    let export: PodiumExport =
        serde_json::from_slice(&bytes).context("could not parse podium export JSON")?;
    Ok((export, input_hash))
}

async fn import_items(
    repository: &StorageRepository<'_>,
    items: &[PodiumExportItem],
    context: ImportContext<'_>,
) -> Result<ImportedItems> {
    let mut imported = ImportedItems::default();
    for (row_index, item) in items.iter().enumerate() {
        import_item(
            repository,
            item,
            i64::try_from(row_index).context("row index does not fit into i64")?,
            context,
            &mut imported,
        )
        .await?;
    }
    Ok(imported)
}

async fn import_item(
    repository: &StorageRepository<'_>,
    item: &PodiumExportItem,
    row_index: i64,
    context: ImportContext<'_>,
    imported: &mut ImportedItems,
) -> Result<()> {
    let source_document_id = repository
        .upsert_source_document(&source_document_from_item(item)?)
        .await?;
    let raw_payload = serde_json::to_string(item)?;
    let canonical_fingerprint = canonical_result_fingerprint(item, context.year);
    let source_fingerprint = source_result_fingerprint(item, context.input_hash, context.year);
    let conflict = repository
        .find_canonical_result(&canonical_fingerprint)
        .await?;
    let conflict_status = conflict_status(
        conflict.as_ref(),
        source_fingerprint.as_str(),
        raw_payload.as_str(),
    );
    if conflict_status == "conflict" {
        imported.conflict_count += 1;
    }

    let parsed_row = parsed_result_row_from_item(
        item,
        context.parser_run_id,
        source_document_id,
        row_index,
        context.input_hash,
        context.year,
        &raw_payload,
        &canonical_fingerprint,
        conflict_status,
        conflict.as_ref().map(|reference| reference.id),
    );
    let stored_row = repository
        .insert_parsed_result_row_once(&parsed_row)
        .await?;
    if stored_row.inserted {
        imported.imported_parser_rows += 1;
    } else {
        imported.skipped_duplicate_parser_rows += 1;
    }

    let club_id = repository.upsert_club(&club_from_item(item)).await?;
    let athlete_id = repository.upsert_athlete(&athlete_from_item(item)).await?;
    let discipline_id = repository
        .upsert_discipline(&discipline_from_item(item))
        .await?;
    let result = result_from_item(
        item,
        context.import_run_id,
        source_document_id,
        stored_row.id,
        context.competition_id,
        athlete_id,
        club_id,
        discipline_id,
        &raw_payload,
        &source_fingerprint,
        &canonical_fingerprint,
        conflict_status,
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

fn export_year(export: &PodiumExport) -> Result<i64> {
    export
        .items
        .iter()
        .find_map(item_year)
        .or_else(|| year_from_path(&export.source_report_path))
        .with_context(|| {
            format!(
                "could not infer competition year from {}",
                export.source_report_path.display()
            )
        })
}

fn item_year(item: &PodiumExportItem) -> Option<i64> {
    item.event_date
        .as_deref()
        .and_then(parse_german_year)
        .or_else(|| first_four_digit_year(&item.event_name))
        .or_else(|| year_from_path(&item.local_path))
}

fn parse_german_year(value: &str) -> Option<i64> {
    NaiveDate::parse_from_str(value, "%d.%m.%Y")
        .ok()
        .map(|date| i64::from(date.year()))
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

fn competition_from_export(export: &PodiumExport, year: i64) -> NewCompetition {
    NewCompetition {
        code: format!("{}-{year}", source_scope(&export.source_name)),
        name: export.items.first().map_or_else(
            || export.source_name.clone(),
            |item| item.event_name.clone(),
        ),
        year,
        scope: source_scope(&export.source_name).to_owned(),
        organizer: Some("NDSB".to_owned()),
        association_code: Some("NDSB".to_owned()),
        country_code: Some("DE".to_owned()),
        date_from: export
            .items
            .iter()
            .filter_map(|item| item.event_date.as_deref())
            .min()
            .map(str::to_owned),
        date_to: export
            .items
            .iter()
            .filter_map(|item| item.event_date.as_deref())
            .max()
            .map(str::to_owned),
    }
}

fn source_scope(source_name: &str) -> &'static str {
    if source_name.contains("deutsche") {
        "DM"
    } else if source_name.contains("kreis") {
        "KM"
    } else {
        "LM"
    }
}

fn import_run_from_export(
    export: &PodiumExport,
    input_path: &Path,
    input_hash: &str,
) -> NewImportRun {
    NewImportRun {
        source_document_id: None,
        source_name: export.source_name.clone(),
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
    export: &PodiumExport,
    import_run_id: i64,
    input_path: &Path,
    input_hash: &str,
) -> NewParserRun {
    NewParserRun {
        import_run_id: Some(import_run_id),
        source_name: export.source_name.clone(),
        source_kind: "podium-export".to_owned(),
        parser_name: PARSER_NAME.to_owned(),
        parser_version: env!("CARGO_PKG_VERSION").to_owned(),
        input_path: input_path.display().to_string(),
        input_hash: input_hash.to_owned(),
        source_report_path: Some(export.source_report_path.display().to_string()),
        export_generated_at: Some(export.generated_at.to_rfc3339()),
        status: "success".to_owned(),
        error: None,
    }
}

fn source_document_from_item(item: &PodiumExportItem) -> Result<NewSourceDocument> {
    Ok(NewSourceDocument {
        source_name: item.source_name.clone(),
        url: item.pdf_url.clone(),
        local_path: Some(item.local_path.display().to_string()),
        sha256: local_file_hash(&item.local_path)?,
        etag: None,
        last_modified: None,
        file_size_bytes: local_file_size(&item.local_path)?,
        classification: "david21".to_owned(),
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

fn club_from_item(item: &PodiumExportItem) -> NewClub {
    NewClub {
        canonical_name: canonical_club_name(item),
        association_code: Some(item.association_code.clone()),
        source: Some(item.source_name.clone()),
    }
}

fn athlete_from_item(item: &PodiumExportItem) -> NewAthlete {
    NewAthlete {
        canonical_name: item.shooter.clone(),
        sort_name: Some(item.shooter.clone()),
    }
}

fn discipline_from_item(item: &PodiumExportItem) -> NewDiscipline {
    NewDiscipline {
        code: item.discipline_code.clone(),
        name: item
            .discipline
            .clone()
            .unwrap_or_else(|| "Unbekannte Disziplin".to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
fn parsed_result_row_from_item(
    item: &PodiumExportItem,
    parser_run_id: i64,
    source_document_id: i64,
    row_index: i64,
    input_hash: &str,
    year: i64,
    raw_payload: &str,
    canonical_fingerprint: &str,
    conflict_status: &str,
    conflict_result_id: Option<i64>,
) -> NewParsedResultRow {
    NewParsedResultRow {
        parser_run_id,
        source_document_id: Some(source_document_id),
        row_index,
        row_fingerprint: parsed_row_fingerprint(input_hash, row_index),
        canonical_fingerprint: canonical_fingerprint.to_owned(),
        source_name: item.source_name.clone(),
        competition_year: year,
        competition_scope: source_scope(&item.source_name).to_owned(),
        result_kind: result_kind(&item.result_kind).to_owned(),
        rank: Some(i64::from(item.rank)),
        score: item.score,
        raw_shooter_name: Some(item.shooter.clone()),
        normalized_shooter_name: Some(item.shooter.clone()),
        raw_club_name: Some(item.club.clone()),
        normalized_club_name: Some(canonical_club_name(item)),
        association_code: Some(item.association_code.clone()),
        raw_discipline: item.discipline.clone(),
        normalized_discipline: item.discipline.clone(),
        discipline_code: item.discipline_code.clone(),
        class_name: item.class_name.clone(),
        event_name: Some(item.event_name.clone()),
        event_date: item.event_date.clone(),
        pdf_url: Some(item.pdf_url.clone()),
        local_path: Some(item.local_path.display().to_string()),
        raw_payload: Some(raw_payload.to_owned()),
        conflict_status: conflict_status.to_owned(),
        conflict_result_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn result_from_item(
    item: &PodiumExportItem,
    import_run_id: i64,
    source_document_id: i64,
    parsed_result_row_id: i64,
    competition_id: i64,
    athlete_id: i64,
    club_id: i64,
    discipline_id: i64,
    raw_payload: &str,
    source_fingerprint: &str,
    canonical_fingerprint: &str,
    conflict_status: &str,
) -> NewResult {
    NewResult {
        import_run_id: Some(import_run_id),
        source_document_id: Some(source_document_id),
        parsed_result_row_id: Some(parsed_result_row_id),
        competition_id,
        athlete_id: Some(athlete_id),
        club_id: Some(club_id),
        discipline_id: Some(discipline_id),
        result_kind: result_kind(&item.result_kind).to_owned(),
        rank: Some(i64::from(item.rank)),
        score: item.score,
        medal: medal_for_rank(item.rank).map(str::to_owned),
        participation_only: false,
        event_class: item.class_name.clone(),
        stage: "result".to_owned(),
        raw_shooter_name: Some(item.shooter.clone()),
        raw_club_name: Some(item.club.clone()),
        raw_discipline: item.discipline.clone(),
        raw_payload: Some(raw_payload.to_owned()),
        source_fingerprint: Some(source_fingerprint.to_owned()),
        canonical_fingerprint: Some(canonical_fingerprint.to_owned()),
        conflict_status: conflict_status.to_owned(),
    }
}

const fn result_kind(kind: &PodiumResultKind) -> &'static str {
    match kind {
        PodiumResultKind::Individual => "individual",
        PodiumResultKind::Team => "team",
    }
}

const fn medal_for_rank(rank: u32) -> Option<&'static str> {
    match rank {
        1 => Some("gold"),
        2 => Some("silver"),
        3 => Some("bronze"),
        _ => None,
    }
}

fn parsed_row_fingerprint(input_hash: &str, row_index: i64) -> String {
    format!("{input_hash}|row|{row_index}")
}

fn source_result_fingerprint(item: &PodiumExportItem, input_hash: &str, year: i64) -> String {
    format!("{input_hash}|{}", canonical_result_fingerprint(item, year))
}

fn canonical_result_fingerprint(item: &PodiumExportItem, year: i64) -> String {
    let discipline = item
        .discipline_code
        .as_deref()
        .or(item.discipline.as_deref())
        .unwrap_or_default();
    [
        &item.source_name,
        &year.to_string(),
        discipline,
        result_kind(&item.result_kind),
        &item.rank.to_string(),
        &item.shooter,
    ]
    .join("|")
}

fn conflict_status(
    existing: Option<&CanonicalResultReference>,
    source_fingerprint: &str,
    raw_payload: &str,
) -> &'static str {
    let Some(existing) = existing else {
        return "none";
    };
    if existing.source_fingerprint.as_deref() == Some(source_fingerprint)
        || existing.raw_payload.as_deref() == Some(raw_payload)
    {
        "none"
    } else {
        "conflict"
    }
}

fn canonical_club_name(item: &PodiumExportItem) -> String {
    if item.canonical_club.trim().is_empty() {
        item.club.clone()
    } else {
        item.canonical_club.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::Utc;

    use super::{
        PodiumImportConfig, PodiumImporter, canonical_result_fingerprint, first_four_digit_year,
        item_year, source_result_fingerprint,
    };
    use crate::export::{ManualReviewPdf, PodiumExport, PodiumExportItem, PodiumResultKind};

    #[test]
    fn extracts_year_from_event_name() {
        assert_eq!(
            first_four_digit_year("Landesmeisterschaft 2026"),
            Some(2026)
        );
    }

    #[test]
    fn extracts_year_from_german_event_date() {
        assert_eq!(item_year(&item()), Some(2026));
    }

    #[test]
    fn source_fingerprints_include_hash_source_year_discipline_rank_and_name() {
        assert_eq!(
            source_result_fingerprint(&item(), "abc", 2026),
            "abc|landesmeisterschaften|2026|1.80.40|individual|2|Soares dos Reis, Maximilian"
        );
    }

    #[test]
    fn canonical_fingerprints_do_not_include_input_hash() {
        assert_eq!(
            canonical_result_fingerprint(&item(), 2026),
            "landesmeisterschaften|2026|1.80.40|individual|2|Soares dos Reis, Maximilian"
        );
    }

    #[tokio::test]
    async fn imports_podium_export_and_skips_duplicates() {
        let dir = std::env::temp_dir().join(format!(
            "pdf-explorer-import-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is after unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir is created");
        let input_path = dir.join("podium-export.json");
        let database_path = dir.join("podium-import.sqlite");
        fs::write(
            &input_path,
            serde_json::to_vec(&export()).expect("export serializes"),
        )
        .expect("export is written");

        let importer = PodiumImporter::new(PodiumImportConfig {
            input_path: input_path.clone(),
            database_path: database_path.clone(),
        });
        let first_report = importer.run().await.expect("first import succeeds");
        let second_report = importer.run().await.expect("second import succeeds");

        assert_eq!(first_report.imported_results, 1);
        assert_eq!(first_report.imported_parser_rows, 1);
        assert_eq!(first_report.skipped_duplicate_results, 0);
        assert_eq!(second_report.imported_results, 0);
        assert_eq!(second_report.imported_parser_rows, 0);
        assert_eq!(second_report.skipped_duplicate_results, 1);
        assert_eq!(second_report.skipped_duplicate_parser_rows, 1);

        let _ = fs::remove_dir_all(dir);
    }

    fn item() -> PodiumExportItem {
        PodiumExportItem {
            source_name: "landesmeisterschaften".to_owned(),
            rank: 2,
            result_kind: PodiumResultKind::Individual,
            shooter: "Soares dos Reis, Maximilian".to_owned(),
            club: "080 Schützenverein Reinfeld 1".to_owned(),
            canonical_club: "Schützenverein Reinfeld".to_owned(),
            association_code: "OD".to_owned(),
            association_name: "Stormarn".to_owned(),
            discipline: Some("KK liegend".to_owned()),
            discipline_code: Some("1.80.40".to_owned()),
            class_name: Some("Herren IV".to_owned()),
            event_name: "Landesmeisterschaft 2026".to_owned(),
            event_date: Some("30.05.2026".to_owned()),
            score: Some(621.7),
            pdf_url: "https://example.test/1.80.40.pdf".to_owned(),
            local_path: "data/archive/2026/landesmeisterschaften/downloads/1.80.40.pdf".into(),
        }
    }

    fn export() -> PodiumExport {
        PodiumExport {
            generated_at: Utc::now(),
            source_report_path: "reports/archive/2026/landesmeisterschaften/crawl-report.json"
                .into(),
            source_name: "landesmeisterschaften".to_owned(),
            focus_association_code: "OD".to_owned(),
            max_place: 3,
            item_count: 1,
            manual_review_count: 0,
            manual_review_pdfs: Vec::<ManualReviewPdf>::new(),
            items: vec![item()],
        }
    }
}
