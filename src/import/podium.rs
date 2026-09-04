use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::export::{PodiumExport, PodiumExportItem, PodiumResultKind};
use crate::storage::{
    CanonicalResultReference, Database, DatabaseConfig, NewAthlete, NewClub, NewClubAlias,
    NewCompetition, NewDiscipline, NewImportRun, NewParsedResultRow, NewParserRun, NewResult,
    NewSourceDocument, NewTeam, NewTeamMember, NewTeamResultMember, StorageRepository,
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
    manual_overrides: &'a ManualOverrideSet,
}

#[derive(Debug, Clone, Copy, Default)]
struct ImportedItems {
    imported_parser_rows: usize,
    skipped_duplicate_parser_rows: usize,
    imported_results: usize,
    skipped_duplicate_results: usize,
    conflict_count: usize,
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
        let manual_overrides = ManualOverrideSet::load(&repository).await?;

        let imported = import_items(
            &repository,
            &export.items,
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
    let parser_club = canonical_club_name(item);
    let canonical_club = context.manual_overrides.club_name(&item.club, &parser_club);
    let canonical_athlete = context.manual_overrides.athlete_name(&item.shooter);
    let canonical_fingerprint =
        canonical_result_fingerprint(item, context.year, &canonical_athlete);
    let source_fingerprint =
        source_result_fingerprint(item, context.input_hash, context.year, &canonical_athlete);
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

    let club_id = repository
        .upsert_club(&club_from_item(item, &canonical_club))
        .await?;
    store_club_aliases(repository, club_id, item, &parser_club).await?;
    let athlete_id = repository
        .upsert_athlete(&athlete_from_name(&canonical_athlete))
        .await?;
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
    store_team_relations(
        repository,
        item,
        row_index,
        context,
        TeamRelationReferences {
            source_document: source_document_id,
            parsed_result_row: stored_row.id,
            result: stored.id,
            club: club_id,
            athlete: athlete_id,
            discipline: discipline_id,
        },
        &canonical_club,
        &canonical_athlete,
        conflict_status,
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct TeamRelationReferences {
    source_document: i64,
    parsed_result_row: i64,
    result: i64,
    club: i64,
    athlete: i64,
    discipline: i64,
}

#[allow(clippy::too_many_arguments)]
async fn store_team_relations(
    repository: &StorageRepository<'_>,
    item: &PodiumExportItem,
    row_index: i64,
    context: ImportContext<'_>,
    ids: TeamRelationReferences,
    canonical_club: &str,
    canonical_athlete: &str,
    conflict_status: &str,
) -> Result<()> {
    if !matches!(item.result_kind, PodiumResultKind::Team) {
        return Ok(());
    }

    let team_id = repository
        .upsert_team(&team_from_item(
            item,
            ids.source_document,
            ids.parsed_result_row,
            context.competition_id,
            ids.club,
            ids.discipline,
            canonical_club,
            context.input_hash,
            context.year,
            conflict_status,
        ))
        .await?;
    let team_member_id = repository
        .upsert_team_member(&team_member_from_item(
            item,
            team_id,
            ids.athlete,
            row_index,
            canonical_athlete,
        ))
        .await?;
    repository
        .upsert_team_result_member(&team_result_member_from_item(
            item,
            team_id,
            ids.result,
            ids.athlete,
            team_member_id,
            row_index,
        ))
        .await?;
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

fn club_from_item(item: &PodiumExportItem, canonical_name: &str) -> NewClub {
    NewClub {
        canonical_name: canonical_name.to_owned(),
        association_code: Some(item.association_code.clone()),
        source: Some(item.source_name.clone()),
    }
}

async fn store_club_aliases(
    repository: &StorageRepository<'_>,
    club_id: i64,
    item: &PodiumExportItem,
    parser_club: &str,
) -> Result<()> {
    for alias in [&item.club, parser_club, item.canonical_club.as_str()] {
        let alias = alias.trim();
        if !alias.is_empty() {
            repository
                .upsert_club_alias(&NewClubAlias {
                    club_id,
                    alias: alias.to_owned(),
                    association_code: Some(item.association_code.clone()),
                    source: Some("podium-export".to_owned()),
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

#[allow(clippy::too_many_arguments)]
fn team_from_item(
    item: &PodiumExportItem,
    source_document_id: i64,
    parsed_result_row_id: i64,
    competition_id: i64,
    club_id: i64,
    discipline_id: i64,
    canonical_club: &str,
    input_hash: &str,
    year: i64,
    conflict_status: &str,
) -> NewTeam {
    let team_number = team_number_from_raw_club(&item.club);
    let canonical_name = canonical_team_name(canonical_club, team_number.as_deref());
    let canonical_fingerprint =
        team_canonical_fingerprint(item, year, &canonical_name, team_number.as_deref());
    NewTeam {
        competition_id,
        club_id: Some(club_id),
        discipline_id: Some(discipline_id),
        source_document_id: Some(source_document_id),
        parsed_result_row_id: Some(parsed_result_row_id),
        canonical_name,
        team_number,
        raw_team_name: Some(item.club.clone()),
        rank: Some(i64::from(item.rank)),
        score: None,
        medal: medal_for_rank(item.rank).map(str::to_owned),
        event_class: item.class_name.clone(),
        source_fingerprint: Some(team_source_fingerprint(input_hash, &canonical_fingerprint)),
        canonical_fingerprint: Some(canonical_fingerprint),
        conflict_status: conflict_status.to_owned(),
    }
}

fn team_member_from_item(
    item: &PodiumExportItem,
    team_id: i64,
    athlete_id: i64,
    row_index: i64,
    canonical_athlete: &str,
) -> NewTeamMember {
    NewTeamMember {
        team_id,
        athlete_id: Some(athlete_id),
        member_order: row_index,
        display_name: canonical_athlete.to_owned(),
        raw_name: Some(item.shooter.clone()),
    }
}

fn team_result_member_from_item(
    item: &PodiumExportItem,
    team_id: i64,
    result_id: i64,
    athlete_id: i64,
    team_member_id: i64,
    row_index: i64,
) -> NewTeamResultMember {
    NewTeamResultMember {
        team_id,
        result_id,
        athlete_id: Some(athlete_id),
        team_member_id: Some(team_member_id),
        member_order: row_index,
        score: item.score,
        medal: medal_for_rank(item.rank).map(str::to_owned),
        raw_name: Some(item.shooter.clone()),
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

fn source_result_fingerprint(
    item: &PodiumExportItem,
    input_hash: &str,
    year: i64,
    canonical_athlete: &str,
) -> String {
    format!(
        "{input_hash}|{}",
        canonical_result_fingerprint(item, year, canonical_athlete)
    )
}

fn canonical_result_fingerprint(
    item: &PodiumExportItem,
    year: i64,
    canonical_athlete: &str,
) -> String {
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
        canonical_athlete,
    ]
    .join("|")
}

fn team_source_fingerprint(input_hash: &str, canonical_fingerprint: &str) -> String {
    format!("{input_hash}|{canonical_fingerprint}")
}

fn team_canonical_fingerprint(
    item: &PodiumExportItem,
    year: i64,
    canonical_team_name: &str,
    team_number: Option<&str>,
) -> String {
    let discipline = item
        .discipline_code
        .as_deref()
        .or(item.discipline.as_deref())
        .unwrap_or_default();
    [
        &item.source_name,
        &year.to_string(),
        discipline,
        "team",
        &item.rank.to_string(),
        canonical_team_name,
        team_number.unwrap_or_default(),
        item.class_name.as_deref().unwrap_or_default(),
    ]
    .join("|")
}

fn canonical_team_name(canonical_club: &str, team_number: Option<&str>) -> String {
    team_number.map_or_else(
        || canonical_club.to_owned(),
        |number| format!("{canonical_club} {number}"),
    )
}

fn team_number_from_raw_club(raw_club: &str) -> Option<String> {
    let suffix = raw_club.split_whitespace().last()?.trim();
    if is_team_number_suffix(suffix) {
        Some(suffix.to_owned())
    } else {
        None
    }
}

fn is_team_number_suffix(value: &str) -> bool {
    let upper = value.trim_matches('.').to_ascii_uppercase();
    let is_numeric = !upper.is_empty() && upper.chars().all(|character| character.is_ascii_digit());
    let is_roman = matches!(
        upper.as_str(),
        "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X"
    );
    is_numeric || is_roman
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
        item_year, source_result_fingerprint, team_number_from_raw_club,
    };
    use crate::export::{ManualReviewPdf, PodiumExport, PodiumExportItem, PodiumResultKind};
    use crate::storage::{Database, DatabaseConfig, NewManualOverride, StorageRepository};

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
            source_result_fingerprint(&item(), "abc", 2026, "Soares dos Reis, Maximilian"),
            "abc|landesmeisterschaften|2026|1.80.40|individual|2|Soares dos Reis, Maximilian"
        );
    }

    #[test]
    fn canonical_fingerprints_do_not_include_input_hash() {
        assert_eq!(
            canonical_result_fingerprint(&item(), 2026, "Soares dos Reis, Maximilian"),
            "landesmeisterschaften|2026|1.80.40|individual|2|Soares dos Reis, Maximilian"
        );
    }

    #[test]
    fn extracts_team_number_from_raw_club_suffix() {
        assert_eq!(
            team_number_from_raw_club("Ahrensburger SchG I"),
            Some("I".to_owned())
        );
        assert_eq!(
            team_number_from_raw_club("080 Schützenverein Reinfeld 1"),
            Some("1".to_owned())
        );
        assert_eq!(team_number_from_raw_club("Schützenverein Reinfeld"), None);
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

    #[tokio::test]
    async fn imports_podium_export_with_manual_name_overrides() {
        let dir = std::env::temp_dir().join(format!(
            "pdf-explorer-import-override-test-{}",
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

        let database = Database::new(DatabaseConfig {
            path: database_path.clone(),
        });
        let pool = database.migrated_pool().await.expect("database migrates");
        let repository = StorageRepository::new(&pool);
        repository
            .upsert_manual_override(&NewManualOverride {
                scope: "global".to_owned(),
                entity_type: "club".to_owned(),
                entity_id: None,
                source_document_id: None,
                parsed_result_row_id: None,
                field_name: "canonical_name".to_owned(),
                old_value: "Schützenverein Reinfeld".to_owned(),
                new_value: "Schützenverein Reinfeld e.V.".to_owned(),
                reason: Some("test correction".to_owned()),
                status: "active".to_owned(),
            })
            .await
            .expect("club override is stored");
        repository
            .upsert_manual_override(&NewManualOverride {
                scope: "global".to_owned(),
                entity_type: "athlete".to_owned(),
                entity_id: None,
                source_document_id: None,
                parsed_result_row_id: None,
                field_name: "canonical_name".to_owned(),
                old_value: "Soares dos Reis, Maximilian".to_owned(),
                new_value: "Soares dos Reis, Max".to_owned(),
                reason: Some("test correction".to_owned()),
                status: "active".to_owned(),
            })
            .await
            .expect("athlete override is stored");
        pool.close().await;

        let importer = PodiumImporter::new(PodiumImportConfig {
            input_path,
            database_path: database_path.clone(),
        });
        importer.run().await.expect("import succeeds");

        let pool = database.migrated_pool().await.expect("database opens");
        let club_name = sqlx::query_scalar::<_, String>("SELECT canonical_name FROM clubs")
            .fetch_one(&pool)
            .await
            .expect("club exists");
        let athlete_name = sqlx::query_scalar::<_, String>("SELECT canonical_name FROM athletes")
            .fetch_one(&pool)
            .await
            .expect("athlete exists");
        let raw_shooter =
            sqlx::query_scalar::<_, String>("SELECT raw_shooter_name FROM parsed_result_rows")
                .fetch_one(&pool)
                .await
                .expect("parsed row exists");
        pool.close().await;

        assert_eq!(club_name, "Schützenverein Reinfeld e.V.");
        assert_eq!(athlete_name, "Soares dos Reis, Max");
        assert_eq!(raw_shooter, "Soares dos Reis, Maximilian");

        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn imports_team_results_as_teams_and_member_medals() {
        let dir = std::env::temp_dir().join(format!(
            "pdf-explorer-import-team-test-{}",
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
            serde_json::to_vec(&team_export()).expect("export serializes"),
        )
        .expect("export is written");

        let importer = PodiumImporter::new(PodiumImportConfig {
            input_path,
            database_path: database_path.clone(),
        });
        importer.run().await.expect("import succeeds");

        let database = Database::new(DatabaseConfig {
            path: database_path.clone(),
        });
        let pool = database.migrated_pool().await.expect("database opens");
        let team_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM teams")
            .fetch_one(&pool)
            .await
            .expect("team count is readable");
        let member_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM team_members")
            .fetch_one(&pool)
            .await
            .expect("team member count is readable");
        let result_member_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM team_result_members")
                .fetch_one(&pool)
                .await
                .expect("team result member count is readable");
        let team = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
            "SELECT canonical_name, team_number, raw_team_name, medal FROM teams",
        )
        .fetch_one(&pool)
        .await
        .expect("team is readable");
        let medal_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM team_result_members WHERE medal = 'silver'",
        )
        .fetch_one(&pool)
        .await
        .expect("medal count is readable");
        pool.close().await;

        assert_eq!(team_count, 1);
        assert_eq!(member_count, 2);
        assert_eq!(result_member_count, 2);
        assert_eq!(team.0, "Ahrensburger Schützengilde I");
        assert_eq!(team.1.as_deref(), Some("I"));
        assert_eq!(team.2.as_deref(), Some("Ahrensburger SchG I"));
        assert_eq!(team.3.as_deref(), Some("silver"));
        assert_eq!(medal_count, 2);

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

    fn team_export() -> PodiumExport {
        let mut first = item();
        first.result_kind = PodiumResultKind::Team;
        first.rank = 2;
        first.shooter = "Mannschaft, Eins".to_owned();
        first.club = "Ahrensburger SchG I".to_owned();
        first.canonical_club = "Ahrensburger Schützengilde".to_owned();
        first.score = Some(200.0);

        let mut second = first.clone();
        second.shooter = "Mannschaft, Zwei".to_owned();
        second.score = Some(199.0);

        PodiumExport {
            generated_at: Utc::now(),
            source_report_path: "reports/archive/2026/landesmeisterschaften/crawl-report.json"
                .into(),
            source_name: "landesmeisterschaften".to_owned(),
            focus_association_code: "OD".to_owned(),
            max_place: 3,
            item_count: 2,
            manual_review_count: 0,
            manual_review_pdfs: Vec::<ManualReviewPdf>::new(),
            items: vec![first, second],
        }
    }
}
