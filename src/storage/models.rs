#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSourceDocument {
    pub source_name: String,
    pub url: String,
    pub local_path: Option<String>,
    pub sha256: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub classification: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportRun {
    pub source_document_id: Option<i64>,
    pub source_name: String,
    pub run_kind: String,
    pub parser_name: Option<String>,
    pub parser_version: Option<String>,
    pub input_path: Option<String>,
    pub input_hash: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCompetition {
    pub code: String,
    pub name: String,
    pub year: i64,
    pub scope: String,
    pub organizer: Option<String>,
    pub association_code: Option<String>,
    pub country_code: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewClub {
    pub canonical_name: String,
    pub association_code: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewClubAlias {
    pub club_id: i64,
    pub alias: String,
    pub association_code: Option<String>,
    pub source: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClubAlias {
    pub club_id: i64,
    pub alias: String,
    pub canonical_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAthlete {
    pub canonical_name: String,
    pub sort_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDiscipline {
    pub code: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewResult {
    pub import_run_id: Option<i64>,
    pub source_document_id: Option<i64>,
    pub parsed_result_row_id: Option<i64>,
    pub competition_id: i64,
    pub athlete_id: Option<i64>,
    pub club_id: Option<i64>,
    pub discipline_id: Option<i64>,
    pub result_kind: String,
    pub rank: Option<i64>,
    pub score: Option<f64>,
    pub medal: Option<String>,
    pub participation_only: bool,
    pub event_class: Option<String>,
    pub stage: String,
    pub raw_shooter_name: Option<String>,
    pub raw_club_name: Option<String>,
    pub raw_discipline: Option<String>,
    pub raw_payload: Option<String>,
    pub source_fingerprint: Option<String>,
    pub canonical_fingerprint: Option<String>,
    pub conflict_status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewTeam {
    pub competition_id: i64,
    pub club_id: Option<i64>,
    pub discipline_id: Option<i64>,
    pub source_document_id: Option<i64>,
    pub parsed_result_row_id: Option<i64>,
    pub canonical_name: String,
    pub team_number: Option<String>,
    pub raw_team_name: Option<String>,
    pub rank: Option<i64>,
    pub score: Option<f64>,
    pub medal: Option<String>,
    pub event_class: Option<String>,
    pub source_fingerprint: Option<String>,
    pub canonical_fingerprint: Option<String>,
    pub conflict_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTeamMember {
    pub team_id: i64,
    pub athlete_id: Option<i64>,
    pub member_order: i64,
    pub display_name: String,
    pub raw_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewTeamResultMember {
    pub team_id: i64,
    pub result_id: i64,
    pub athlete_id: Option<i64>,
    pub team_member_id: Option<i64>,
    pub member_order: i64,
    pub score: Option<f64>,
    pub medal: Option<String>,
    pub raw_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewParserRun {
    pub import_run_id: Option<i64>,
    pub source_name: String,
    pub source_kind: String,
    pub parser_name: String,
    pub parser_version: String,
    pub input_path: String,
    pub input_hash: String,
    pub source_report_path: Option<String>,
    pub export_generated_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewParsedResultRow {
    pub parser_run_id: i64,
    pub source_document_id: Option<i64>,
    pub row_index: i64,
    pub row_fingerprint: String,
    pub canonical_fingerprint: String,
    pub source_name: String,
    pub competition_year: i64,
    pub competition_scope: String,
    pub result_kind: String,
    pub rank: Option<i64>,
    pub score: Option<f64>,
    pub raw_shooter_name: Option<String>,
    pub normalized_shooter_name: Option<String>,
    pub raw_club_name: Option<String>,
    pub normalized_club_name: Option<String>,
    pub association_code: Option<String>,
    pub raw_discipline: Option<String>,
    pub normalized_discipline: Option<String>,
    pub discipline_code: Option<String>,
    pub class_name: Option<String>,
    pub event_name: Option<String>,
    pub event_date: Option<String>,
    pub pdf_url: Option<String>,
    pub local_path: Option<String>,
    pub raw_payload: Option<String>,
    pub conflict_status: String,
    pub conflict_result_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewManualOverride {
    pub scope: String,
    pub entity_type: String,
    pub entity_id: Option<i64>,
    pub source_document_id: Option<i64>,
    pub parsed_result_row_id: Option<i64>,
    pub field_name: String,
    pub old_value: String,
    pub new_value: String,
    pub reason: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManualOverride {
    pub id: i64,
    pub scope: String,
    pub entity_type: String,
    pub field_name: String,
    pub old_value: String,
    pub new_value: String,
    pub reason: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCounts {
    pub source_documents: i64,
    pub import_runs: i64,
    pub competitions: i64,
    pub clubs: i64,
    pub club_aliases: i64,
    pub athletes: i64,
    pub disciplines: i64,
    pub results: i64,
    pub parser_runs: i64,
    pub parsed_result_rows: i64,
    pub manual_overrides: i64,
    pub teams: i64,
    pub team_members: i64,
    pub team_result_members: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredResult {
    pub id: i64,
    pub inserted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredParsedResultRow {
    pub id: i64,
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResultReference {
    pub id: i64,
    pub source_fingerprint: Option<String>,
    pub raw_payload: Option<String>,
}
