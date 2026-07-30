use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use pdf_explorer::export::{
    CombinedExportConfig, CombinedExporter, ParticipationExportConfig, ParticipationExporter,
    PodiumExportConfig, PodiumExporter,
};
use pdf_explorer::ingest::{CrawlConfig, CrawlReporter};
use pdf_explorer::pdf::{ExtractOptions, PdfExtractor};
use pdf_explorer::sport_results::SportResultsParser;

const DEFAULT_SOURCE_NAME: &str = "default";
const DEFAULT_DOWNLOAD_DIR: &str = "data/downloads";
const DEFAULT_MANUAL_REVIEW_DIR: &str = "data/manual-review";
const DEFAULT_CRAWL_REPORT: &str = "reports/latest-crawl-report.json";
const DEFAULT_CRAWL_HTML_REPORT: &str = "reports/latest-crawl-report.html";

#[derive(Debug, Parser)]
#[command(name = "pdf-explorer")]
#[command(about = "PDF parsing workspace: text extraction first, OCR-ready later.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
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
    /// Remove generated state, downloads, manual-review files, and reports.
    Clean(CleanArgs),
}

#[derive(Debug, Parser)]
struct CrawlReportArgs {
    /// Source page URL to inspect for PDF links.
    url: String,

    /// Stable origin label stored in reports and used for source-specific state.
    #[arg(long, default_value = DEFAULT_SOURCE_NAME)]
    source_name: String,

    /// Directory for downloaded PDFs and local state.
    #[arg(long, default_value = ".pdf-explorer")]
    state_dir: PathBuf,

    /// Directory for downloaded PDFs.
    #[arg(long, default_value = DEFAULT_DOWNLOAD_DIR)]
    download_dir: PathBuf,

    /// Directory for PDFs that need manual format review.
    #[arg(long, default_value = DEFAULT_MANUAL_REVIEW_DIR)]
    manual_review_dir: PathBuf,

    /// JSON report path.
    #[arg(long, default_value = DEFAULT_CRAWL_REPORT)]
    report: PathBuf,

    /// HTML report path for manual review.
    #[arg(long, default_value = DEFAULT_CRAWL_HTML_REPORT)]
    html_report: PathBuf,

    /// Focus region. Stored in the report and later usable for filtering/export steering.
    #[arg(long, default_value = "Stormarn")]
    focus: String,

    /// Association/Kreis code used to identify focus rows in DAVID21+ files.
    #[arg(long, default_value = "OD")]
    focus_association_code: String,

    /// Limit discovery to a year section such as 2025 or 2026.
    #[arg(long)]
    year: Option<String>,

    /// Additional PDF URLs that should be included in this crawl besides discovered links.
    #[arg(long = "extra-pdf-url")]
    extra_pdf_urls: Vec<String>,

    /// Follow same-host HTML links up to this depth while searching for PDFs.
    #[arg(long, default_value_t = 1)]
    max_depth: usize,

    /// Limit the number of HTML pages visited during discovery.
    #[arg(long, default_value_t = 25)]
    max_pages: usize,

    /// Treat PDFs with very little extracted text as OCR candidates.
    #[arg(long, default_value_t = 80)]
    min_text_chars: usize,
}

#[derive(Debug, Parser)]
struct ExportPodiumArgs {
    /// JSON crawl report created by crawl-report.
    #[arg(long, default_value = "reports/latest-crawl-report.json")]
    crawl_report: PathBuf,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-podium-export.json")]
    output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-podium-export.html")]
    html_output: PathBuf,

    /// Association/Kreis code to export, or "all" to filter by Kreis in the HTML report.
    #[arg(long, default_value = "OD")]
    focus_association_code: String,

    /// Highest rank to include for team and individual results.
    #[arg(long, default_value_t = 3)]
    max_place: u32,

    /// Treat PDFs with very little extracted text as OCR candidates.
    #[arg(long, default_value_t = 80)]
    min_text_chars: usize,
}

#[derive(Debug, Parser)]
struct ExportParticipationArgs {
    /// Crawl report used to build the known focus club list.
    #[arg(long, default_value = "reports/ndsb-2025-crawl-report.json")]
    club_source_report: PathBuf,

    /// Crawl report whose PDFs should be checked for participation.
    #[arg(long, default_value = "reports/ndsb-2025-dm-crawl-report.json")]
    results_report: PathBuf,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-participation-export.json")]
    output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-participation-export.html")]
    html_output: PathBuf,

    /// Association/Kreis code used to build the known club list.
    #[arg(long, default_value = "OD")]
    focus_association_code: String,

    /// Treat PDFs with very little extracted text as OCR candidates.
    #[arg(long, default_value_t = 80)]
    min_text_chars: usize,
}

#[derive(Debug, Parser)]
struct ExportCombinedArgs {
    /// JSON podium export created by export-podium.
    #[arg(long, default_value = "reports/ndsb-2025-podium-export.json")]
    podium_export: PathBuf,

    /// JSON participation export created by export-participation.
    #[arg(long, default_value = "reports/ndsb-2025-dm-participation-export.json")]
    participation_export: PathBuf,

    /// JSON output path for downstream processing.
    #[arg(long, default_value = "reports/latest-combined-export.json")]
    output: PathBuf,

    /// HTML output path for manual review.
    #[arg(long, default_value = "reports/latest-combined-export.html")]
    html_output: PathBuf,
}

#[derive(Debug, Parser)]
#[allow(clippy::struct_field_names)]
struct CleanArgs {
    /// Directory containing crawler state and the manifest.
    #[arg(long, default_value = ".pdf-explorer")]
    state_dir: PathBuf,

    /// Directory containing downloaded PDFs.
    #[arg(long, default_value = "data/downloads")]
    download_dir: PathBuf,

    /// Directory containing PDFs queued for manual review.
    #[arg(long, default_value = "data/manual-review")]
    manual_review_dir: PathBuf,

    /// Directory containing generated reports and exports.
    #[arg(long, default_value = "reports")]
    report_dir: PathBuf,

    /// Directory containing year/source archived downloads.
    #[arg(long, default_value = "data/archive")]
    archive_data_dir: PathBuf,

    /// Directory containing temporary generated files.
    #[arg(long, default_value = "tmp")]
    tmp_dir: PathBuf,
}

#[derive(Clone, Debug, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[allow(clippy::too_many_lines)]
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse {
            input,
            format,
            min_text_chars,
        } => {
            let extractor = PdfExtractor::new(ExtractOptions { min_text_chars });
            let document = extractor
                .extract(&input)
                .with_context(|| format!("failed to parse PDF at {}", input.display()))?;

            match format {
                OutputFormat::Text => {
                    println!("{}", document.text.trim());
                    if document.needs_ocr {
                        eprintln!(
                            "warning: extracted text is sparse; this PDF may need OCR ({}/{} chars threshold)",
                            document.text.len(),
                            min_text_chars
                        );
                    }
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&document)?);
                }
            }
        }
        Commands::ParseSport {
            input,
            min_text_chars,
        } => {
            let extractor = PdfExtractor::new(ExtractOptions { min_text_chars });
            let document = extractor
                .extract(&input)
                .with_context(|| format!("failed to parse PDF at {}", input.display()))?;
            let result_list = SportResultsParser::new().parse(&document.text)?;

            println!("{}", serde_json::to_string_pretty(&result_list)?);
        }
        Commands::CrawlReport(args) => {
            let CrawlReportArgs {
                url,
                mut source_name,
                state_dir,
                mut download_dir,
                mut manual_review_dir,
                mut report,
                mut html_report,
                focus,
                focus_association_code,
                year,
                extra_pdf_urls,
                max_depth,
                max_pages,
                min_text_chars,
            } = *args;
            source_name = resolved_source_name(&source_name, &url);
            apply_crawl_archive_defaults(
                year.as_deref(),
                &source_name,
                &mut download_dir,
                &mut manual_review_dir,
                &mut report,
                &mut html_report,
            );
            let config = CrawlConfig {
                source_url: url,
                source_name,
                state_dir,
                download_dir,
                manual_review_dir,
                report_path: report,
                html_report_path: html_report,
                focus,
                focus_association_code,
                year,
                extra_pdf_urls,
                max_depth,
                max_pages,
                min_text_chars,
            };
            let report = CrawlReporter::new(config)?.run()?;

            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::ExportPodium(args) => {
            let ExportPodiumArgs {
                crawl_report,
                output,
                html_output,
                focus_association_code,
                max_place,
                min_text_chars,
            } = *args;
            let config = PodiumExportConfig {
                crawl_report_path: crawl_report,
                json_output_path: output,
                html_output_path: html_output,
                focus_association_code,
                max_place,
                min_text_chars,
            };
            let export = PodiumExporter::new(config).run()?;

            println!("{}", serde_json::to_string_pretty(&export)?);
        }
        Commands::ExportParticipation(args) => {
            let ExportParticipationArgs {
                club_source_report,
                results_report,
                output,
                html_output,
                focus_association_code,
                min_text_chars,
            } = *args;
            let config = ParticipationExportConfig {
                club_source_report_path: club_source_report,
                results_report_path: results_report,
                json_output_path: output,
                html_output_path: html_output,
                focus_association_code,
                min_text_chars,
            };
            let export = ParticipationExporter::new(config).run()?;

            println!("{}", serde_json::to_string_pretty(&export)?);
        }
        Commands::ExportCombined(args) => {
            let ExportCombinedArgs {
                podium_export,
                participation_export,
                output,
                html_output,
            } = *args;
            let config = CombinedExportConfig {
                podium_export_path: podium_export,
                participation_export_path: participation_export,
                json_output_path: output,
                html_output_path: html_output,
            };
            let export = CombinedExporter::new(config).run()?;

            println!("{}", serde_json::to_string_pretty(&export)?);
        }
        Commands::Clean(args) => {
            clean_generated_data(&args)?;
        }
    }

    Ok(())
}

fn clean_generated_data(args: &CleanArgs) -> anyhow::Result<()> {
    for path in [
        &args.state_dir,
        &args.download_dir,
        &args.manual_review_dir,
        &args.report_dir,
        &args.archive_data_dir,
        &args.tmp_dir,
    ] {
        remove_path_if_exists(path)?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        println!("skipped {}", path.display());
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory {}", path.display()))?;
    } else {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove file {}", path.display()))?;
    }
    println!("removed {}", path.display());
    Ok(())
}

fn resolved_source_name(source_name: &str, url: &str) -> String {
    if source_name != DEFAULT_SOURCE_NAME {
        return source_name.to_string();
    }

    infer_source_name(url).unwrap_or_else(|| source_name.to_string())
}

fn infer_source_name(url: &str) -> Option<String> {
    if url.contains("landesmeisterschaften") {
        Some("landesmeisterschaften".to_string())
    } else if url.contains("deutsche-meisterschaften") {
        Some("deutsche-meisterschaften".to_string())
    } else {
        None
    }
}

fn apply_crawl_archive_defaults(
    year: Option<&str>,
    source_name: &str,
    download_dir: &mut PathBuf,
    manual_review_dir: &mut PathBuf,
    report: &mut PathBuf,
    html_report: &mut PathBuf,
) {
    let Some(year) = year.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let source_slug = archive_slug(source_name);

    if download_dir == Path::new(DEFAULT_DOWNLOAD_DIR) {
        *download_dir = PathBuf::from("data")
            .join("archive")
            .join(year)
            .join(&source_slug)
            .join("downloads");
    }
    if manual_review_dir == Path::new(DEFAULT_MANUAL_REVIEW_DIR) {
        *manual_review_dir = PathBuf::from("data")
            .join("archive")
            .join(year)
            .join(&source_slug)
            .join("manual-review");
    }
    if report == Path::new(DEFAULT_CRAWL_REPORT) {
        *report = PathBuf::from("reports")
            .join("archive")
            .join(year)
            .join(&source_slug)
            .join("crawl-report.json");
    }
    if html_report == Path::new(DEFAULT_CRAWL_HTML_REPORT) {
        *html_report = PathBuf::from("reports")
            .join("archive")
            .join(year)
            .join(&source_slug)
            .join("crawl-report.html");
    }
}

fn archive_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator && !slug.is_empty() {
            slug.push('-');
            previous_was_separator = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_CRAWL_HTML_REPORT, DEFAULT_CRAWL_REPORT, DEFAULT_DOWNLOAD_DIR,
        DEFAULT_MANUAL_REVIEW_DIR, apply_crawl_archive_defaults, archive_slug,
        resolved_source_name,
    };
    use std::path::PathBuf;

    #[test]
    fn derives_archive_paths_for_year_and_source_defaults() {
        let mut download_dir = PathBuf::from(DEFAULT_DOWNLOAD_DIR);
        let mut manual_review_dir = PathBuf::from(DEFAULT_MANUAL_REVIEW_DIR);
        let mut report = PathBuf::from(DEFAULT_CRAWL_REPORT);
        let mut html_report = PathBuf::from(DEFAULT_CRAWL_HTML_REPORT);

        apply_crawl_archive_defaults(
            Some("2026"),
            "deutsche-meisterschaften",
            &mut download_dir,
            &mut manual_review_dir,
            &mut report,
            &mut html_report,
        );

        assert_eq!(
            download_dir,
            PathBuf::from("data/archive/2026/deutsche-meisterschaften/downloads")
        );
        assert_eq!(
            manual_review_dir,
            PathBuf::from("data/archive/2026/deutsche-meisterschaften/manual-review")
        );
        assert_eq!(
            report,
            PathBuf::from("reports/archive/2026/deutsche-meisterschaften/crawl-report.json")
        );
        assert_eq!(
            html_report,
            PathBuf::from("reports/archive/2026/deutsche-meisterschaften/crawl-report.html")
        );
    }

    #[test]
    fn keeps_custom_paths_when_archiving() {
        let mut download_dir = PathBuf::from("custom/downloads");
        let mut manual_review_dir = PathBuf::from("custom/manual");
        let mut report = PathBuf::from("custom/report.json");
        let mut html_report = PathBuf::from("custom/report.html");

        apply_crawl_archive_defaults(
            Some("2026"),
            "landesmeisterschaften",
            &mut download_dir,
            &mut manual_review_dir,
            &mut report,
            &mut html_report,
        );

        assert_eq!(download_dir, PathBuf::from("custom/downloads"));
        assert_eq!(manual_review_dir, PathBuf::from("custom/manual"));
        assert_eq!(report, PathBuf::from("custom/report.json"));
        assert_eq!(html_report, PathBuf::from("custom/report.html"));
    }

    #[test]
    fn infers_source_name_from_known_ndsb_urls() {
        assert_eq!(
            resolved_source_name(
                "default",
                "https://www.ndsb-sh.de/sport/deutsche-meisterschaften"
            ),
            "deutsche-meisterschaften"
        );
        assert_eq!(
            archive_slug("Deutsche Meisterschaften"),
            "deutsche-meisterschaften"
        );
    }
}
