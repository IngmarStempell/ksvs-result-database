use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use pdf_explorer::export::{
    CombinedExportConfig, CombinedExporter, DatabaseCombinedExportConfig, DatabaseCombinedExporter,
    DatabasePodiumExportConfig, DatabasePodiumExporter, ManualNameOverrides,
    ParticipationExportConfig, ParticipationExporter, PodiumExportConfig, PodiumExporter,
};
use pdf_explorer::import::{
    ParticipationImportConfig, ParticipationImporter, PodiumImportConfig, PodiumImporter,
};
use pdf_explorer::ingest::{CrawlConfig, CrawlReporter};
use pdf_explorer::pdf::{ExtractOptions, PdfExtractor};
use pdf_explorer::sport_results::SportResultsParser;
use pdf_explorer::storage::{Database, DatabaseConfig, NewManualOverride, StorageRepository};
use pdf_explorer::web::{WebConfig, WebServer};

use crate::cli::{
    CleanArgs, Cli, Commands, CrawlReportArgs, DEFAULT_CRAWL_HTML_REPORT, DEFAULT_CRAWL_REPORT,
    DEFAULT_DOWNLOAD_DIR, DEFAULT_MANUAL_REVIEW_DIR, DEFAULT_SOURCE_NAME, DbArgs, DbCommands,
    ManualOverrideCommands, ManualOverrideValueArgs, OutputFormat,
};

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse {
            input,
            format,
            min_text_chars,
        } => parse_pdf(&input, format, min_text_chars),
        Commands::ParseSport {
            input,
            min_text_chars,
        } => parse_sport(&input, min_text_chars),
        Commands::CrawlReport(args) => crawl_report(*args),
        Commands::ExportPodium(args) => export_podium(*args).await,
        Commands::ExportParticipation(args) => export_participation(*args),
        Commands::ExportCombined(args) => export_combined(*args),
        Commands::ExportDbPodium(args) => export_db_podium(*args).await,
        Commands::ExportDbCombined(args) => export_db_combined(*args).await,
        Commands::ImportPodium(args) => import_podium(args).await,
        Commands::ImportParticipation(args) => import_participation(args).await,
        Commands::ManualOverride(args) => manage_manual_overrides(args).await,
        Commands::Clean(args) => clean_generated_data(&args),
        Commands::Db(args) => manage_database(args).await,
        Commands::Serve(args) => serve_web_ui(args).await,
    }
}

fn parse_pdf(input: &Path, format: OutputFormat, min_text_chars: usize) -> anyhow::Result<()> {
    let extractor = PdfExtractor::new(ExtractOptions { min_text_chars });
    let document = extractor
        .extract(input)
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

    Ok(())
}

fn parse_sport(input: &Path, min_text_chars: usize) -> anyhow::Result<()> {
    let extractor = PdfExtractor::new(ExtractOptions { min_text_chars });
    let document = extractor
        .extract(input)
        .with_context(|| format!("failed to parse PDF at {}", input.display()))?;
    let result_list = SportResultsParser::new().parse(&document.text)?;

    println!("{}", serde_json::to_string_pretty(&result_list)?);
    Ok(())
}

fn crawl_report(args: CrawlReportArgs) -> anyhow::Result<()> {
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
    } = args;
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
    Ok(())
}

async fn export_podium(args: crate::cli::ExportPodiumArgs) -> anyhow::Result<()> {
    let manual_overrides = if let Some(path) = args.override_database.as_deref() {
        load_manual_name_overrides(path).await?
    } else {
        ManualNameOverrides::default()
    };
    let config = PodiumExportConfig {
        crawl_report_path: args.crawl_report,
        json_output_path: args.output,
        html_output_path: args.html_output,
        focus_association_code: args.focus_association_code,
        max_place: args.max_place,
        min_text_chars: args.min_text_chars,
        manual_overrides,
    };
    let export = PodiumExporter::new(config).run()?;

    println!("{}", serde_json::to_string_pretty(&export)?);
    Ok(())
}

async fn load_manual_name_overrides(path: &Path) -> anyhow::Result<ManualNameOverrides> {
    let database = Database::new(DatabaseConfig {
        path: path.to_path_buf(),
    });
    let pool = database.migrated_pool().await?;
    let repository = StorageRepository::new(&pool);
    let club_names = repository
        .active_manual_overrides("club", "canonical_name")
        .await?
        .into_iter()
        .map(|manual_override| (manual_override.old_value, manual_override.new_value))
        .collect();
    let athlete_names = repository
        .active_manual_overrides("athlete", "canonical_name")
        .await?
        .into_iter()
        .map(|manual_override| (manual_override.old_value, manual_override.new_value))
        .collect();
    pool.close().await;

    Ok(ManualNameOverrides {
        club_names,
        athlete_names,
    })
}

fn export_participation(args: crate::cli::ExportParticipationArgs) -> anyhow::Result<()> {
    let config = ParticipationExportConfig {
        club_source_report_path: args.club_source_report,
        results_report_path: args.results_report,
        json_output_path: args.output,
        html_output_path: args.html_output,
        focus_association_code: args.focus_association_code,
        min_text_chars: args.min_text_chars,
    };
    let export = ParticipationExporter::new(config).run()?;

    println!("{}", serde_json::to_string_pretty(&export)?);
    Ok(())
}

fn export_combined(args: crate::cli::ExportCombinedArgs) -> anyhow::Result<()> {
    let config = CombinedExportConfig {
        podium_export_path: args.podium_export,
        participation_export_path: args.participation_export,
        json_output_path: args.output,
        html_output_path: args.html_output,
    };
    let export = CombinedExporter::new(config).run()?;

    println!("{}", serde_json::to_string_pretty(&export)?);
    Ok(())
}

async fn export_db_podium(args: crate::cli::ExportDbPodiumArgs) -> anyhow::Result<()> {
    let config = DatabasePodiumExportConfig {
        database_path: args.database,
        json_output_path: args.output,
        html_output_path: args.html_output,
        year: args.year,
        competition_scope: args.competition_scope,
        focus_association_code: args.focus_association_code,
        max_place: args.max_place,
    };
    let export = DatabasePodiumExporter::new(config).run().await?;

    println!("{}", serde_json::to_string_pretty(&export)?);
    Ok(())
}

async fn export_db_combined(args: crate::cli::ExportDbCombinedArgs) -> anyhow::Result<()> {
    let config = DatabaseCombinedExportConfig {
        database_path: args.database,
        json_output_path: args.output,
        html_output_path: args.html_output,
        year: args.year,
        focus_association_code: args.focus_association_code,
        max_place: args.max_place,
    };
    let export = DatabaseCombinedExporter::new(config).run().await?;

    println!("{}", serde_json::to_string_pretty(&export)?);
    Ok(())
}

async fn import_podium(args: crate::cli::ImportPodiumArgs) -> anyhow::Result<()> {
    let report = PodiumImporter::new(PodiumImportConfig {
        input_path: args.input,
        database_path: args.database,
    })
    .run()
    .await?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn import_participation(args: crate::cli::ImportParticipationArgs) -> anyhow::Result<()> {
    let report = ParticipationImporter::new(ParticipationImportConfig {
        input_path: args.input,
        database_path: args.database,
    })
    .run()
    .await?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn manage_manual_overrides(args: crate::cli::ManualOverrideArgs) -> anyhow::Result<()> {
    match args.command {
        ManualOverrideCommands::AddClub(args) => add_manual_override("club", args).await,
        ManualOverrideCommands::AddAthlete(args) => add_manual_override("athlete", args).await,
        ManualOverrideCommands::List(args) => {
            let database = Database::new(DatabaseConfig {
                path: args.database.clone(),
            });
            let pool = database.migrated_pool().await?;
            let repository = StorageRepository::new(&pool);
            let overrides = repository.all_active_manual_overrides().await?;
            pool.close().await;

            println!("{}", serde_json::to_string_pretty(&overrides)?);
            Ok(())
        }
    }
}

async fn add_manual_override(
    entity_type: &str,
    args: ManualOverrideValueArgs,
) -> anyhow::Result<()> {
    let database = Database::new(DatabaseConfig {
        path: args.database.clone(),
    });
    let pool = database.migrated_pool().await?;
    let repository = StorageRepository::new(&pool);
    let id = repository
        .upsert_manual_override(&NewManualOverride {
            scope: "global".to_owned(),
            entity_type: entity_type.to_owned(),
            entity_id: None,
            source_document_id: None,
            parsed_result_row_id: None,
            field_name: "canonical_name".to_owned(),
            old_value: args.from,
            new_value: args.to,
            reason: args.reason,
            status: "active".to_owned(),
        })
        .await?;
    pool.close().await;

    println!("stored manual override {id}");
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

async fn manage_database(args: DbArgs) -> anyhow::Result<()> {
    match args.command {
        DbCommands::Init(args) => {
            let database = Database::new(DatabaseConfig {
                path: args.database.clone(),
            });
            database.init().await?;
            println!("initialized database {}", args.database.display());
        }
        DbCommands::Migrate(args) => {
            let database = Database::new(DatabaseConfig {
                path: args.database.clone(),
            });
            let report = database.migrate().await?;
            println!(
                "migrated database {} ({} applied migrations recorded)",
                report.database_path.display(),
                report.applied_migration_count
            );
        }
    }
    Ok(())
}

async fn serve_web_ui(args: crate::cli::ServeArgs) -> anyhow::Result<()> {
    WebServer::new(WebConfig {
        database_path: args.database,
        bind: args.bind,
    })
    .run()
    .await
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
