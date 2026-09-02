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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCounts {
    pub source_documents: i64,
    pub import_runs: i64,
    pub competitions: i64,
    pub clubs: i64,
    pub athletes: i64,
    pub disciplines: i64,
    pub results: i64,
}
