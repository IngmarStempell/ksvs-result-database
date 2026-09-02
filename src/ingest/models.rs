use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct CrawlConfig {
    pub source_url: String,
    pub source_name: String,
    pub state_dir: PathBuf,
    pub download_dir: PathBuf,
    pub manual_review_dir: PathBuf,
    pub report_path: PathBuf,
    pub html_report_path: PathBuf,
    pub focus: String,
    pub focus_association_code: String,
    pub year: Option<String>,
    pub extra_pdf_urls: Vec<String>,
    pub max_depth: usize,
    pub max_pages: usize,
    pub min_text_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
    pub generated_at: DateTime<Utc>,
    pub source_url: String,
    #[serde(default)]
    pub source_name: String,
    pub focus: String,
    pub focus_association_code: String,
    pub discovered_pdf_count: usize,
    pub downloaded_count: usize,
    pub changed_count: usize,
    pub unchanged_count: usize,
    pub removed_count: usize,
    pub auto_processed_count: usize,
    pub manual_review_count: usize,
    pub failed_count: usize,
    pub removed_pdfs: Vec<RemovedPdf>,
    pub pdfs: Vec<PdfReportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovedPdf {
    pub url: String,
    pub previous_local_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfReportItem {
    pub url: String,
    pub status: PdfChangeStatus,
    pub classification: PdfClassification,
    pub local_path: Option<PathBuf>,
    pub manual_review_path: Option<PathBuf>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sha256: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub text_char_count: Option<usize>,
    pub needs_ocr: Option<bool>,
    pub david21_summary: Option<David21Summary>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfChangeStatus {
    New,
    Changed,
    Unchanged,
    Removed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfClassification {
    David21,
    ManualReview,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct David21Summary {
    pub event_name: String,
    pub event_date: Option<String>,
    pub location: Option<String>,
    pub discipline_code: Option<String>,
    pub discipline: Option<String>,
    pub class_name: Option<String>,
    pub team_results: usize,
    pub team_members: usize,
    pub individual_results: usize,
    pub out_of_competition_team_results: usize,
    pub out_of_competition_individual_results: usize,
    pub associations: Vec<String>,
    pub podium_associations: Vec<String>,
    pub association_placements: Vec<AssociationPlacement>,
    pub focus: David21FocusSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociationPlacement {
    pub association_code: String,
    pub place: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct David21FocusSummary {
    pub association_code: String,
    pub team_results: usize,
    pub team_members: usize,
    pub individual_results: usize,
    pub out_of_competition_team_results: usize,
    pub out_of_competition_team_members: usize,
    pub out_of_competition_individual_results: usize,
    pub podium_team_results: usize,
    pub podium_team_members: usize,
    pub podium_individual_results: usize,
    pub podium_clubs: Vec<String>,
    pub clubs: Vec<String>,
}
