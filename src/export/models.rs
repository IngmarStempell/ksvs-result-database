use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct PodiumExportConfig {
    pub crawl_report_path: PathBuf,
    pub json_output_path: PathBuf,
    pub html_output_path: PathBuf,
    pub focus_association_code: String,
    pub max_place: u32,
    pub min_text_chars: usize,
}

#[derive(Debug, Clone)]
pub struct ParticipationExportConfig {
    pub club_source_report_path: PathBuf,
    pub results_report_path: PathBuf,
    pub json_output_path: PathBuf,
    pub html_output_path: PathBuf,
    pub focus_association_code: String,
    pub min_text_chars: usize,
}

#[derive(Debug, Clone)]
pub struct CombinedExportConfig {
    pub podium_export_path: PathBuf,
    pub participation_export_path: PathBuf,
    pub json_output_path: PathBuf,
    pub html_output_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodiumExport {
    pub generated_at: DateTime<Utc>,
    pub source_report_path: PathBuf,
    pub source_name: String,
    pub focus_association_code: String,
    pub max_place: u32,
    pub item_count: usize,
    #[serde(default)]
    pub manual_review_count: usize,
    #[serde(default)]
    pub manual_review_pdfs: Vec<ManualReviewPdf>,
    pub items: Vec<PodiumExportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualReviewPdf {
    pub url: String,
    pub reason: Option<String>,
    pub text_char_count: Option<usize>,
    pub needs_ocr: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodiumResultKind {
    Individual,
    Team,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodiumExportItem {
    pub source_name: String,
    pub rank: u32,
    pub result_kind: PodiumResultKind,
    pub shooter: String,
    pub club: String,
    #[serde(default)]
    pub canonical_club: String,
    pub association_code: String,
    #[serde(default)]
    pub association_name: String,
    pub discipline: Option<String>,
    pub discipline_code: Option<String>,
    pub class_name: Option<String>,
    pub event_name: String,
    pub event_date: Option<String>,
    #[serde(default)]
    pub score: Option<f64>,
    pub pdf_url: String,
    pub local_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipationExport {
    pub generated_at: DateTime<Utc>,
    pub club_source_report_path: PathBuf,
    pub results_report_path: PathBuf,
    pub club_source_name: String,
    pub results_source_name: String,
    pub focus_association_code: String,
    pub known_club_count: usize,
    pub matched_club_count: usize,
    pub match_count: usize,
    pub known_clubs: Vec<String>,
    pub matches: Vec<ParticipationMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipationMatch {
    pub club: String,
    #[serde(default)]
    pub canonical_club: String,
    #[serde(default)]
    pub shooters: Vec<String>,
    pub source_name: String,
    pub pdf_url: String,
    pub local_path: PathBuf,
    pub text_char_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedExport {
    pub generated_at: DateTime<Utc>,
    pub podium_export_path: PathBuf,
    pub participation_export_path: PathBuf,
    pub focus_association_code: String,
    pub club_count: usize,
    pub podium_item_count: usize,
    pub participation_match_count: usize,
    pub clubs: Vec<CombinedClub>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedClub {
    pub club: String,
    pub podium_items: Vec<PodiumExportItem>,
    pub participation_matches: Vec<ParticipationMatch>,
}
