use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

pub const DEFAULT_SOURCE_NAME: &str = "default";
pub const DEFAULT_DOWNLOAD_DIR: &str = "data/downloads";
pub const DEFAULT_MANUAL_REVIEW_DIR: &str = "data/manual-review";
pub const DEFAULT_CRAWL_REPORT: &str = "reports/latest-crawl-report.json";
pub const DEFAULT_CRAWL_HTML_REPORT: &str = "reports/latest-crawl-report.html";
pub const DEFAULT_DATABASE_PATH: &str = "data/pdf-explorer.sqlite";

#[derive(Debug, Parser)]
#[command(name = "pdf-explorer")]
#[command(about = "PDF parsing workspace: text extraction first, OCR-ready later.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Extract text and basic metadata from a PDF.
    Parse {
        /// Path to the PDF file.
        input: PathBuf,

        /// Output format.
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,

        /// Treat PDFs with very little extracted text as OCR candidates.
        #[arg(long, default_value_t = 80)]
        min_text_chars: usize,
    },
    /// Extract DAVID21+ sport result lists into structured JSON.
    ParseSport {
        /// Path to the PDF file.
        input: PathBuf,

        /// Treat PDFs with very little extracted text as OCR candidates.
        #[arg(long, default_value_t = 80)]
        min_text_chars: usize,
    },
    /// Follow a page, download linked PDFs, detect changes, classify them, and write a report.
    CrawlReport(Box<CrawlReportArgs>),
    /// Export filtered podium shooters from a crawl report into JSON and HTML.
    ExportPodium(Box<ExportPodiumArgs>),
    /// Match known focus clubs against another source where participation matters.
    ExportParticipation(Box<ExportParticipationArgs>),
    /// Combine podium and participation exports into one club-oriented JSON and HTML report.
    ExportCombined(Box<ExportCombinedArgs>),
    /// Export podium JSON and HTML from the local database.
    ExportDbPodium(Box<ExportDbPodiumArgs>),
    /// Export combined LM medal and DM participation JSON and HTML from the local database.
    ExportDbCombined(Box<ExportDbCombinedArgs>),
    /// Import a podium export JSON into the local database.
    ImportPodium(ImportPodiumArgs),
    /// Import a participation export JSON into the local database.
    ImportParticipation(ImportParticipationArgs),
    /// Manage manual name corrections used by database imports.
    ManualOverride(ManualOverrideArgs),
    /// Remove generated state, downloads, manual-review files, and reports.
    Clean(CleanArgs),
    /// Manage the local `SQLite` database.
    Db(DbArgs),
}

#[derive(Debug, Parser)]
pub struct CrawlReportArgs {
    /// Source page URL to inspect for PDF links.
    pub url: String,

    /// Stable origin label stored in reports and used for source-specific state.
    #[arg(long, default_value = DEFAULT_SOURCE_NAME)]
    pub source_name: String,

    /// Directory for downloaded PDFs and local state.
    #[arg(long, default_value = ".pdf-explorer")]
    pub state_dir: PathBuf,

    /// Directory for downloaded PDFs.
    #[arg(long, default_value = DEFAULT_DOWNLOAD_DIR)]
    pub download_dir: PathBuf,

    /// Directory for PDFs that need manual format review.
    #[arg(long, default_value = DEFAULT_MANUAL_REVIEW_DIR)]
    pub manual_review_dir: PathBuf,

    /// JSON report path.
    #[arg(long, default_value = DEFAULT_CRAWL_REPORT)]
    pub report: PathBuf,

    /// HTML report path for manual review.
    #[arg(long, default_value = DEFAULT_CRAWL_HTML_REPORT)]
    pub html_report: PathBuf,

    /// Focus region. Stored in the report and later usable for filtering/export steering.
    #[arg(long, default_value = "Stormarn")]
    pub focus: String,

    /// Association/Kreis code used to identify focus rows in DAVID21+ files.
    #[arg(long, default_value = "OD")]
    pub focus_association_code: String,

    /// Limit discovery to a year section such as 2025 or 2026.
    #[arg(long)]
    pub year: Option<String>,

    /// Additional PDF URLs that should be included in this crawl besides discovered links.
    #[arg(long = "extra-pdf-url")]
    pub extra_pdf_urls: Vec<String>,

    /// Follow same-host HTML links up to this depth while searching for PDFs.
    #[arg(long, default_value_t = 1)]
    pub max_depth: usize,

    /// Limit the number of HTML pages visited during discovery.
    #[arg(long, default_value_t = 25)]
    pub max_pages: usize,

    /// Treat PDFs with very little extracted text as OCR candidates.
    #[arg(long, default_value_t = 80)]
    pub min_text_chars: usize,
}

#[derive(Debug, Parser)]
pub struct ExportPodiumArgs {
    /// JSON crawl report created by crawl-report.
    #[arg(long, default_value = "reports/latest-crawl-report.json")]
    pub crawl_report: PathBuf,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-podium-export.json")]
    pub output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-podium-export.html")]
    pub html_output: PathBuf,

    /// Association/Kreis code to export, or "all" to filter by Kreis in the HTML report.
    #[arg(long, default_value = "OD")]
    pub focus_association_code: String,

    /// Default highest rank shown in the HTML report.
    #[arg(long, default_value_t = 3)]
    pub max_place: u32,

    /// Treat PDFs with very little extracted text as OCR candidates.
    #[arg(long, default_value_t = 80)]
    pub min_text_chars: usize,

    /// `SQLite` database file containing active manual name corrections.
    #[arg(long)]
    pub override_database: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub struct ExportParticipationArgs {
    /// Crawl report used to build the known focus club list.
    #[arg(long, default_value = "reports/ndsb-2025-crawl-report.json")]
    pub club_source_report: PathBuf,

    /// Crawl report whose PDFs should be checked for participation.
    #[arg(long, default_value = "reports/ndsb-2025-dm-crawl-report.json")]
    pub results_report: PathBuf,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-participation-export.json")]
    pub output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-participation-export.html")]
    pub html_output: PathBuf,

    /// Association/Kreis code used to build the known club list.
    #[arg(long, default_value = "OD")]
    pub focus_association_code: String,

    /// Treat PDFs with very little extracted text as OCR candidates.
    #[arg(long, default_value_t = 80)]
    pub min_text_chars: usize,
}

#[derive(Debug, Parser)]
pub struct ExportCombinedArgs {
    /// JSON podium export created by export-podium.
    #[arg(long, default_value = "reports/ndsb-2025-podium-export.json")]
    pub podium_export: PathBuf,

    /// JSON participation export created by export-participation.
    #[arg(long, default_value = "reports/ndsb-2025-dm-participation-export.json")]
    pub participation_export: PathBuf,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-combined-export.json")]
    pub output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-combined-export.html")]
    pub html_output: PathBuf,
}

#[derive(Debug, Parser)]
pub struct ExportDbPodiumArgs {
    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,

    /// Competition year to export.
    #[arg(long)]
    pub year: i64,

    /// Competition scope to export, for example LM, DM, or KM.
    #[arg(long, default_value = "LM")]
    pub competition_scope: String,

    /// Association/Kreis code to export, or "all".
    #[arg(long, default_value = "OD")]
    pub focus_association_code: String,

    /// Highest rank exported from the database.
    #[arg(long, default_value_t = 3)]
    pub max_place: u32,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-db-podium-export.json")]
    pub output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-db-podium-export.html")]
    pub html_output: PathBuf,
}

#[derive(Debug, Parser)]
pub struct ExportDbCombinedArgs {
    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,

    /// Competition year to export.
    #[arg(long)]
    pub year: i64,

    /// Association/Kreis code to export, or "all".
    #[arg(long, default_value = "OD")]
    pub focus_association_code: String,

    /// Highest LM rank exported from the database.
    #[arg(long, default_value_t = 3)]
    pub max_place: u32,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-db-combined-export.json")]
    pub output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-db-combined-export.html")]
    pub html_output: PathBuf,
}

#[derive(Debug, Parser)]
pub struct ImportPodiumArgs {
    /// JSON podium export created by export-podium.
    #[arg(long, default_value = "reports/latest-podium-export.json")]
    pub input: PathBuf,

    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
}

#[derive(Debug, Parser)]
pub struct ImportParticipationArgs {
    /// JSON participation export created by export-participation.
    #[arg(long, default_value = "reports/latest-participation-export.json")]
    pub input: PathBuf,

    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
}

#[derive(Debug, Parser)]
pub struct ManualOverrideArgs {
    #[command(subcommand)]
    pub command: ManualOverrideCommands,
}

#[derive(Debug, Subcommand)]
pub enum ManualOverrideCommands {
    /// Store a manual club name correction.
    AddClub(ManualOverrideValueArgs),
    /// Store a manual athlete name correction.
    AddAthlete(ManualOverrideValueArgs),
    /// List active manual corrections.
    List(ManualOverrideListArgs),
}

#[derive(Debug, Parser)]
pub struct ManualOverrideValueArgs {
    /// Value produced by the parser/export.
    #[arg(long)]
    pub from: String,

    /// Canonical value that should be used from now on.
    #[arg(long)]
    pub to: String,

    /// Optional note explaining the correction.
    #[arg(long)]
    pub reason: Option<String>,

    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
}

#[derive(Debug, Parser)]
pub struct ManualOverrideListArgs {
    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
}

#[derive(Debug, Parser)]
#[allow(clippy::struct_field_names)]
pub struct CleanArgs {
    /// Directory containing crawler state and the manifest.
    #[arg(long, default_value = ".pdf-explorer")]
    pub state_dir: PathBuf,

    /// Directory containing downloaded PDFs.
    #[arg(long, default_value = "data/downloads")]
    pub download_dir: PathBuf,

    /// Directory containing PDFs queued for manual review.
    #[arg(long, default_value = "data/manual-review")]
    pub manual_review_dir: PathBuf,

    /// Directory containing generated reports and exports.
    #[arg(long, default_value = "reports")]
    pub report_dir: PathBuf,

    /// Directory containing year/source archived downloads.
    #[arg(long, default_value = "data/archive")]
    pub archive_data_dir: PathBuf,

    /// Directory containing temporary generated files.
    #[arg(long, default_value = "tmp")]
    pub tmp_dir: PathBuf,
}

#[derive(Debug, Parser)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommands,
}

#[derive(Debug, Subcommand)]
pub enum DbCommands {
    /// Create the `SQLite` database file if it does not exist.
    Init(DatabaseArgs),
    /// Apply all pending database migrations.
    Migrate(DatabaseArgs),
}

#[derive(Debug, Parser)]
pub struct DatabaseArgs {
    /// `SQLite` database file used by the application.
    #[arg(long, default_value = DEFAULT_DATABASE_PATH)]
    pub database: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}
