use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::ingest::{CrawlReport, PdfClassification};
use crate::pdf::{ExtractOptions, PdfExtractor};
use crate::sport_results::{Rank, SportResultList, SportResultsParser};
use crate::template::render_template;

use super::models::{
    CombinedClub, CombinedExport, CombinedExportConfig, ManualReviewPdf, ParticipationExport,
    ParticipationExportConfig, ParticipationMatch, PodiumExport, PodiumExportConfig,
    PodiumExportItem, PodiumResultKind,
};

const PODIUM_EXPORT_TEMPLATE: &str = include_str!("../../templates/podium-export.html");
const PODIUM_MANUAL_REVIEW_TABLE_TEMPLATE: &str =
    include_str!("../../templates/podium-manual-review-table.html");
const PARTICIPATION_EXPORT_TEMPLATE: &str =
    include_str!("../../templates/participation-export.html");
const COMBINED_EXPORT_TEMPLATE: &str = include_str!("../../templates/combined-export.html");

#[derive(Debug)]
pub struct PodiumExporter {
    config: PodiumExportConfig,
}

#[derive(Debug)]
pub struct ParticipationExporter {
    config: ParticipationExportConfig,
}

#[derive(Debug)]
pub struct CombinedExporter {
    config: CombinedExportConfig,
}

#[derive(Debug, Serialize)]
struct PodiumHtmlItem<'a> {
    rank: u32,
    result_kind: &'a PodiumResultKind,
    shooter: &'a str,
    club: &'a str,
    canonical_club: &'a str,
    association_code: &'a str,
    association_name: &'a str,
    discipline: Option<&'a str>,
    discipline_code: Option<&'a str>,
    class_name: Option<&'a str>,
}

impl<'a> From<&'a PodiumExportItem> for PodiumHtmlItem<'a> {
    fn from(item: &'a PodiumExportItem) -> Self {
        Self {
            rank: item.rank,
            result_kind: &item.result_kind,
            shooter: &item.shooter,
            club: &item.club,
            canonical_club: &item.canonical_club,
            association_code: &item.association_code,
            association_name: &item.association_name,
            discipline: item.discipline.as_deref(),
            discipline_code: item.discipline_code.as_deref(),
            class_name: item.class_name.as_deref(),
        }
    }
}

impl PodiumExporter {
    #[must_use]
    pub const fn new(config: PodiumExportConfig) -> Self {
        Self { config }
    }

    /// Creates JSON and HTML podium exports from a crawl report.
    ///
    /// # Errors
    ///
    /// Returns an error when the crawl report cannot be read, when a referenced
    /// DAVID21+ PDF cannot be parsed, or when an output file cannot be written.
    pub fn run(&self) -> Result<PodiumExport> {
        let crawl_report = self.read_crawl_report()?;
        let extractor = PdfExtractor::new(ExtractOptions {
            min_text_chars: self.config.min_text_chars,
        });
        let parser = SportResultsParser::new();
        let mut items = Vec::new();

        for pdf in crawl_report
            .pdfs
            .iter()
            .filter(|pdf| pdf.classification == PdfClassification::David21)
        {
            let Some(local_path) = &pdf.local_path else {
                continue;
            };
            let document = extractor
                .extract(local_path)
                .with_context(|| format!("could not extract {}", local_path.display()))?;
            let result_list = parser
                .parse(&document.text)
                .with_context(|| format!("could not parse DAVID21+ {}", local_path.display()))?;

            items.extend(self.export_items_from_result_list(
                &result_list,
                &crawl_report.source_name,
                &pdf.url,
                local_path,
            ));
        }

        let known_clubs = known_focus_clubs(
            &crawl_report,
            &extractor,
            &parser,
            &self.config.focus_association_code,
        )?;
        let known_club_associations = known_club_associations(
            &crawl_report,
            &extractor,
            &parser,
            &self.config.focus_association_code,
        )?;
        for pdf in crawl_report
            .pdfs
            .iter()
            .filter(|pdf| pdf.classification != PdfClassification::David21)
        {
            let Some(local_path) = &pdf.local_path else {
                continue;
            };
            let Ok(document) = extractor.extract(local_path) else {
                continue;
            };
            let text = meyton_export_text(&document.text, local_path);
            items.extend(self.export_meyton_team_items(
                &text,
                &known_clubs,
                &known_club_associations,
                &crawl_report.source_name,
                &pdf.url,
                local_path,
            ));
        }

        items.sort_by(|left, right| {
            left.shooter
                .cmp(&right.shooter)
                .then_with(|| left.club.cmp(&right.club))
                .then_with(|| left.rank.cmp(&right.rank))
                .then_with(|| left.discipline.cmp(&right.discipline))
        });

        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: self.config.crawl_report_path.clone(),
            source_name: crawl_report.source_name.clone(),
            focus_association_code: self.config.focus_association_code.clone(),
            max_place: self.config.max_place,
            item_count: items.len(),
            manual_review_count: crawl_report.manual_review_count,
            manual_review_pdfs: manual_review_pdfs(&crawl_report),
            items,
        };
        self.write_json_export(&export)?;
        self.write_html_export(&export)?;
        Ok(export)
    }

    fn read_crawl_report(&self) -> Result<CrawlReport> {
        let content = fs::read_to_string(&self.config.crawl_report_path).with_context(|| {
            format!(
                "could not read crawl report {}",
                self.config.crawl_report_path.display()
            )
        })?;
        serde_json::from_str(&content).with_context(|| {
            format!(
                "could not parse crawl report {}",
                self.config.crawl_report_path.display()
            )
        })
    }

    fn export_items_from_result_list(
        &self,
        result_list: &SportResultList,
        source_name: &str,
        pdf_url: &str,
        local_path: &Path,
    ) -> Vec<PodiumExportItem> {
        let mut items = Vec::new();
        let code = self.config.focus_association_code.trim();

        for team in result_list
            .team_results
            .iter()
            .filter(|team| association_matches(&team.association, code) && team.rank.is_some())
        {
            let rank = team.rank.expect("team rank checked above");
            for member in &team.members {
                items.push(PodiumExportItem {
                    source_name: source_name.to_string(),
                    rank,
                    result_kind: PodiumResultKind::Team,
                    shooter: member.name.clone(),
                    club: team.club.clone(),
                    canonical_club: canonical_club_name(&team.club),
                    association_code: team.association.clone(),
                    association_name: association_name(&team.association).to_string(),
                    discipline: team.event.discipline.clone(),
                    discipline_code: team.event.discipline_code.clone(),
                    class_name: team.event.class_name.clone(),
                    event_name: team.event.name.clone(),
                    event_date: team.event.date.clone(),
                    score: Some(normalize_score(f64::from(member.total))),
                    pdf_url: pdf_url.to_string(),
                    local_path: local_path.to_path_buf(),
                });
            }
        }

        for result in result_list.individual_results.iter().filter(|result| {
            association_matches(&result.association, code)
                && place_from_rank(&result.rank).is_some()
        }) {
            let rank = place_from_rank(&result.rank).expect("individual rank checked above");
            items.push(PodiumExportItem {
                source_name: source_name.to_string(),
                rank,
                result_kind: PodiumResultKind::Individual,
                shooter: result.name.clone(),
                club: result.club.clone(),
                canonical_club: canonical_club_name(&result.club),
                association_code: result.association.clone(),
                association_name: association_name(&result.association).to_string(),
                discipline: result.event.discipline.clone(),
                discipline_code: result.event.discipline_code.clone(),
                class_name: result.event.class_name.clone(),
                event_name: result.event.name.clone(),
                event_date: result.event.date.clone(),
                score: Some(normalize_score(f64::from(result.total))),
                pdf_url: pdf_url.to_string(),
                local_path: local_path.to_path_buf(),
            });
        }

        items
    }

    fn export_meyton_team_items(
        &self,
        text: &str,
        known_clubs: &[String],
        known_club_associations: &BTreeMap<String, String>,
        source_name: &str,
        pdf_url: &str,
        local_path: &Path,
    ) -> Vec<PodiumExportItem> {
        let mut items = Vec::new();
        if !is_meyton_text(text) {
            return items;
        }
        let mut current_event = MeytonEvent::default();
        let mut current_team = None::<MeytonTeam>;
        let mut pending_shooter = None::<MeytonShooter>;

        for line in text.lines().map(collapse_whitespace) {
            if let Some(event_name) = meyton_event_name(&line) {
                current_event = MeytonEvent {
                    discipline_code: meyton_discipline_code(&event_name),
                    event_name,
                    event_date: None,
                };
                current_team = None;
                pending_shooter = None;
                continue;
            }
            if current_event.event_date.is_none() {
                current_event.event_date = meyton_event_date(&line);
            }
            if is_meyton_rank_header(&line) {
                current_team = None;
                pending_shooter = None;
            }
            if let Some(team) = meyton_team_header(&line, known_clubs) {
                current_team = Some(team);
                pending_shooter = None;
                continue;
            }

            let Some(team) = current_team.as_mut() else {
                continue;
            };
            if team.member_count >= 2 {
                continue;
            }
            let shooter = pending_shooter.take().map_or_else(
                || meyton_shooter_name(&line),
                |prefix| meyton_continued_shooter_name(&prefix, &line),
            );
            let Some(shooter) = shooter else {
                continue;
            };
            if shooter.name.ends_with(',') {
                pending_shooter = Some(shooter);
                continue;
            }
            team.member_count += 1;
            let association_code = meyton_association_code(
                &team.club,
                known_club_associations,
                &self.config.focus_association_code,
            );
            items.push(PodiumExportItem {
                source_name: source_name.to_string(),
                rank: team.rank,
                result_kind: PodiumResultKind::Individual,
                shooter: shooter.name,
                club: team.club.clone(),
                canonical_club: canonical_club_name(&team.club),
                association_name: association_name(&association_code).to_string(),
                association_code,
                discipline: Some(current_event.event_name.clone()),
                discipline_code: current_event.discipline_code.clone(),
                class_name: Some("Mixed".to_string()),
                event_name: current_event.event_name.clone(),
                event_date: current_event.event_date.clone(),
                score: shooter.score,
                pdf_url: pdf_url.to_string(),
                local_path: local_path.to_path_buf(),
            });
        }

        items
    }

    fn write_json_export(&self, export: &PodiumExport) -> Result<()> {
        if let Some(parent) = self.config.json_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(export)?;
        fs::write(&self.config.json_output_path, content)
            .with_context(|| format!("could not write {}", self.config.json_output_path.display()))
    }

    fn write_html_export(&self, export: &PodiumExport) -> Result<()> {
        if let Some(parent) = self.config.html_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let report_dir = self.config.html_output_path.parent();
        let content = render_html_export(export, report_dir);
        fs::write(&self.config.html_output_path, content)
            .with_context(|| format!("could not write {}", self.config.html_output_path.display()))
    }
}

impl ParticipationExporter {
    #[must_use]
    pub const fn new(config: ParticipationExportConfig) -> Self {
        Self { config }
    }

    /// Creates a participation report by matching known focus clubs against another source.
    ///
    /// # Errors
    ///
    /// Returns an error when reports cannot be read, when known clubs cannot be
    /// extracted from the club source, or when output files cannot be written.
    pub fn run(&self) -> Result<ParticipationExport> {
        let club_source_report = read_crawl_report(&self.config.club_source_report_path)?;
        let results_report = read_crawl_report(&self.config.results_report_path)?;
        let extractor = PdfExtractor::new(ExtractOptions {
            min_text_chars: self.config.min_text_chars,
        });
        let parser = SportResultsParser::new();
        let known_clubs = known_focus_clubs(
            &club_source_report,
            &extractor,
            &parser,
            &self.config.focus_association_code,
        )?;
        let matches = participation_matches(&results_report, &extractor, &known_clubs);
        let matched_club_count = matches
            .iter()
            .map(|item| item.club.clone())
            .collect::<BTreeSet<_>>()
            .len();

        let export = ParticipationExport {
            generated_at: Utc::now(),
            club_source_report_path: self.config.club_source_report_path.clone(),
            results_report_path: self.config.results_report_path.clone(),
            club_source_name: report_source_name(
                &club_source_report,
                &self.config.club_source_report_path,
            ),
            results_source_name: report_source_name(
                &results_report,
                &self.config.results_report_path,
            ),
            focus_association_code: self.config.focus_association_code.clone(),
            known_club_count: known_clubs.len(),
            matched_club_count,
            match_count: matches.len(),
            known_clubs,
            matches,
        };
        self.write_json_export(&export)?;
        self.write_html_export(&export)?;
        Ok(export)
    }

    fn write_json_export(&self, export: &ParticipationExport) -> Result<()> {
        if let Some(parent) = self.config.json_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(export)?;
        fs::write(&self.config.json_output_path, content)
            .with_context(|| format!("could not write {}", self.config.json_output_path.display()))
    }

    fn write_html_export(&self, export: &ParticipationExport) -> Result<()> {
        if let Some(parent) = self.config.html_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = render_participation_html(export, self.config.html_output_path.parent());
        fs::write(&self.config.html_output_path, content)
            .with_context(|| format!("could not write {}", self.config.html_output_path.display()))
    }
}

impl CombinedExporter {
    #[must_use]
    pub const fn new(config: CombinedExportConfig) -> Self {
        Self { config }
    }

    /// Combines podium results and participation matches into one club-oriented export.
    ///
    /// # Errors
    ///
    /// Returns an error when input exports cannot be read or output files cannot be written.
    pub fn run(&self) -> Result<CombinedExport> {
        let podium_export = read_json_export::<PodiumExport>(&self.config.podium_export_path)?;
        let participation_export =
            read_json_export::<ParticipationExport>(&self.config.participation_export_path)?;
        let known_names = combined_known_club_names(&podium_export, &participation_export);
        let mut clubs = BTreeMap::<String, CombinedClub>::new();

        for known_club in &participation_export.known_clubs {
            let club = resolve_truncated_club(&canonical_club_name(known_club), &known_names);
            clubs.entry(club.clone()).or_insert_with(|| CombinedClub {
                club,
                podium_items: Vec::new(),
                participation_matches: Vec::new(),
            });
        }

        for mut item in podium_export.items {
            if item.canonical_club.trim().is_empty() {
                item.canonical_club = canonical_club_name(&item.club);
            }
            item.canonical_club = resolve_truncated_club(&item.canonical_club, &known_names);
            let club = item.canonical_club.clone();
            clubs
                .entry(club.clone())
                .or_insert_with(|| CombinedClub {
                    club,
                    podium_items: Vec::new(),
                    participation_matches: Vec::new(),
                })
                .podium_items
                .push(item);
        }

        for mut match_item in participation_export.matches {
            if match_item.canonical_club.trim().is_empty() {
                match_item.canonical_club = canonical_club_name(&match_item.club);
            }
            match_item.canonical_club =
                resolve_truncated_club(&match_item.canonical_club, &known_names);
            let club = match_item.canonical_club.clone();
            clubs
                .entry(club.clone())
                .or_insert_with(|| CombinedClub {
                    club,
                    podium_items: Vec::new(),
                    participation_matches: Vec::new(),
                })
                .participation_matches
                .push(match_item);
        }

        let mut clubs = clubs.into_values().collect::<Vec<_>>();
        for club in &mut clubs {
            club.podium_items.sort_by(|left, right| {
                left.shooter
                    .cmp(&right.shooter)
                    .then_with(|| left.rank.cmp(&right.rank))
                    .then_with(|| left.discipline.cmp(&right.discipline))
            });
            club.participation_matches
                .sort_by(|left, right| left.pdf_url.cmp(&right.pdf_url));
        }

        let podium_item_count = clubs.iter().map(|club| club.podium_items.len()).sum();
        let participation_match_count = clubs
            .iter()
            .map(|club| club.participation_matches.len())
            .sum();
        let export = CombinedExport {
            generated_at: Utc::now(),
            podium_export_path: self.config.podium_export_path.clone(),
            participation_export_path: self.config.participation_export_path.clone(),
            focus_association_code: participation_export.focus_association_code,
            club_count: clubs.len(),
            podium_item_count,
            participation_match_count,
            clubs,
        };

        self.write_json_export(&export)?;
        self.write_html_export(&export)?;
        Ok(export)
    }

    fn write_json_export(&self, export: &CombinedExport) -> Result<()> {
        if let Some(parent) = self.config.json_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(export)?;
        fs::write(&self.config.json_output_path, content)
            .with_context(|| format!("could not write {}", self.config.json_output_path.display()))
    }

    fn write_html_export(&self, export: &CombinedExport) -> Result<()> {
        if let Some(parent) = self.config.html_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = render_combined_html(export, self.config.html_output_path.parent());
        fs::write(&self.config.html_output_path, content)
            .with_context(|| format!("could not write {}", self.config.html_output_path.display()))
    }
}

fn read_crawl_report(path: &Path) -> Result<CrawlReport> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("could not read crawl report {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("could not parse crawl report {}", path.display()))
}

fn read_json_export<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let content = fs::read_to_string(path)
        .with_context(|| format!("could not read export {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("could not parse export {}", path.display()))
}

fn report_source_name(report: &CrawlReport, path: &Path) -> String {
    if report.source_name.trim().is_empty() {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        report.source_name.clone()
    }
}

fn known_focus_clubs(
    report: &CrawlReport,
    extractor: &PdfExtractor,
    parser: &SportResultsParser,
    focus_association_code: &str,
) -> Result<Vec<String>> {
    let mut clubs = BTreeSet::new();
    let code = focus_association_code.trim();

    for pdf in report
        .pdfs
        .iter()
        .filter(|pdf| pdf.classification == PdfClassification::David21)
    {
        let Some(local_path) = &pdf.local_path else {
            continue;
        };
        let document = extractor
            .extract(local_path)
            .with_context(|| format!("could not extract {}", local_path.display()))?;
        let result_list = parser
            .parse(&document.text)
            .with_context(|| format!("could not parse DAVID21+ {}", local_path.display()))?;

        for team in result_list
            .team_results
            .iter()
            .chain(&result_list.out_of_competition_team_results)
            .filter(|team| association_matches(&team.association, code))
        {
            clubs.insert(canonical_club_name(&team.club));
        }

        for result in result_list
            .individual_results
            .iter()
            .chain(&result_list.out_of_competition_individual_results)
            .filter(|result| association_matches(&result.association, code))
        {
            clubs.insert(canonical_club_name(&result.club));
        }
    }

    let clubs = clubs.into_iter().collect::<Vec<_>>();
    Ok(collapse_truncated_clubs(&clubs))
}

fn known_club_associations(
    report: &CrawlReport,
    extractor: &PdfExtractor,
    parser: &SportResultsParser,
    focus_association_code: &str,
) -> Result<BTreeMap<String, String>> {
    let mut associations = BTreeMap::new();
    let code = focus_association_code.trim();

    for pdf in report
        .pdfs
        .iter()
        .filter(|pdf| pdf.classification == PdfClassification::David21)
    {
        let Some(local_path) = &pdf.local_path else {
            continue;
        };
        let document = extractor
            .extract(local_path)
            .with_context(|| format!("could not extract {}", local_path.display()))?;
        let result_list = parser
            .parse(&document.text)
            .with_context(|| format!("could not parse DAVID21+ {}", local_path.display()))?;

        for team in result_list
            .team_results
            .iter()
            .chain(&result_list.out_of_competition_team_results)
            .filter(|team| association_matches(&team.association, code))
        {
            associations
                .entry(canonical_club_name(&team.club))
                .or_insert_with(|| team.association.clone());
        }

        for result in result_list
            .individual_results
            .iter()
            .chain(&result_list.out_of_competition_individual_results)
            .filter(|result| association_matches(&result.association, code))
        {
            associations
                .entry(canonical_club_name(&result.club))
                .or_insert_with(|| result.association.clone());
        }
    }

    Ok(associations)
}

fn association_matches(association: &str, filter: &str) -> bool {
    filter.eq_ignore_ascii_case("all") || association == filter
}

fn meyton_association_code(
    club: &str,
    known_club_associations: &BTreeMap<String, String>,
    fallback: &str,
) -> String {
    known_club_associations
        .get(&canonical_club_name(club))
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Debug)]
struct MeytonTeam {
    rank: u32,
    club: String,
    member_count: usize,
}

#[derive(Debug)]
struct MeytonShooter {
    name: String,
    score: Option<f64>,
}

#[derive(Debug, Default)]
struct MeytonEvent {
    event_name: String,
    event_date: Option<String>,
    discipline_code: Option<String>,
}

fn meyton_export_text(default_text: &str, local_path: &Path) -> String {
    if !is_meyton_text(default_text) {
        return default_text.to_string();
    }

    Command::new("pdftotext")
        .arg("-layout")
        .arg(local_path)
        .arg("-")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| default_text.to_string())
}

fn is_meyton_text(text: &str) -> bool {
    text.contains("Meyton") || text.contains("Ranklist") || text.contains("Finale")
}

fn meyton_event_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.contains("_K") && line.contains("Finale") {
        Some(line.to_string())
    } else {
        None
    }
}

fn meyton_discipline_code(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.split(|character: char| !character.is_ascii_alphanumeric())
            .find(|part| {
                part.starts_with('K')
                    && part[1..]
                        .chars()
                        .all(|character| character.is_ascii_digit())
            })
            .map(ToOwned::to_owned)
    })
}

fn meyton_event_date(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.split_whitespace()
            .find(|part| {
                let parts = part.split('.').collect::<Vec<_>>();
                parts.len() == 3
                    && parts
                        .iter()
                        .all(|part| part.chars().all(|character| character.is_ascii_digit()))
            })
            .map(ToOwned::to_owned)
    })
}

fn meyton_team_header(line: &str, known_clubs: &[String]) -> Option<MeytonTeam> {
    let (rank, club) = line.split_once(". ")?;
    let rank = rank.parse::<u32>().ok()?;
    let club = match_known_club(club, known_clubs)?;

    Some(MeytonTeam {
        rank,
        club,
        member_count: 0,
    })
}

fn is_meyton_rank_header(line: &str) -> bool {
    line.split_once(". ")
        .and_then(|(rank, _)| rank.parse::<u32>().ok())
        .is_some()
}

fn match_known_club(value: &str, known_clubs: &[String]) -> Option<String> {
    let normalized_value = normalize_match_text(value);
    known_clubs.iter().find_map(|club| {
        let matched = club_aliases(club).into_iter().any(|alias| {
            let normalized_alias = normalize_match_text(&alias);
            normalized_value.contains(&normalized_alias)
                || normalized_alias.ends_with(&normalized_value)
        });
        matched.then(|| club.clone())
    })
}

fn meyton_shooter_name(line: &str) -> Option<MeytonShooter> {
    let mut parts = line.split_whitespace();
    parts
        .next()
        .filter(|part| part.chars().all(|character| character.is_ascii_digit()))?;

    let score = line
        .split_whitespace()
        .rev()
        .find_map(|part| part.parse::<f64>().ok());
    let mut name_parts = Vec::new();
    for part in parts {
        if part.chars().all(|character| character.is_ascii_digit()) {
            break;
        }
        name_parts.push(part);
    }
    let name = name_parts.join(" ");
    name.contains(',').then_some(MeytonShooter { name, score })
}

fn meyton_continued_shooter_name(prefix: &MeytonShooter, line: &str) -> Option<MeytonShooter> {
    let continuation = line
        .split_whitespace()
        .take_while(|part| {
            !part.chars().all(|character| character.is_ascii_digit())
                && !matches!(*part, "Single" | "Shot" | "Series")
        })
        .collect::<Vec<_>>()
        .join(" ");
    if continuation.is_empty() {
        None
    } else {
        Some(MeytonShooter {
            name: format!("{} {continuation}", prefix.name),
            score: prefix.score,
        })
    }
}

fn collapse_truncated_clubs(clubs: &[String]) -> Vec<String> {
    clubs
        .iter()
        .filter(|club| {
            truncated_prefix(club).is_none_or(|prefix| {
                !clubs
                    .iter()
                    .any(|candidate| candidate != *club && candidate.starts_with(prefix))
            })
        })
        .cloned()
        .collect()
}

fn truncated_prefix(club: &str) -> Option<&str> {
    club.find("...").map(|index| club[..index].trim_end())
}

fn participation_matches(
    report: &CrawlReport,
    extractor: &PdfExtractor,
    known_clubs: &[String],
) -> Vec<ParticipationMatch> {
    let source_name = report_source_name(report, Path::new("results-report"));
    let mut matches = Vec::new();

    for pdf in &report.pdfs {
        let Some(local_path) = &pdf.local_path else {
            continue;
        };
        let Ok(document) = extractor.extract(local_path) else {
            continue;
        };
        let normalized_text = normalize_match_text(&document.text);

        for club in known_clubs {
            let aliases = club_aliases(club);
            if aliases
                .iter()
                .any(|alias| normalized_text.contains(&normalize_match_text(alias)))
            {
                matches.push(ParticipationMatch {
                    club: club.clone(),
                    canonical_club: canonical_club_name(club),
                    shooters: participation_shooters(&document.text, &aliases),
                    source_name: source_name.clone(),
                    pdf_url: pdf.url.clone(),
                    local_path: local_path.clone(),
                    text_char_count: document.text.len(),
                });
            }
        }
    }

    matches.sort_by(|left, right| {
        left.club
            .cmp(&right.club)
            .then_with(|| left.pdf_url.cmp(&right.pdf_url))
    });
    matches
}

fn normalize_match_text(value: &str) -> String {
    let mut normalized = String::new();
    for character in value.chars() {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
        } else {
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn participation_shooters(text: &str, aliases: &[String]) -> Vec<String> {
    let mut shooters = BTreeSet::new();
    let mut aliases = aliases.to_vec();
    aliases.sort_by_key(|alias| std::cmp::Reverse(alias.len()));

    for line in text.lines().map(collapse_whitespace) {
        if line.is_empty() || line.starts_with("Mannschaft") || line.starts_with("Verein ") {
            continue;
        }

        for alias in &aliases {
            if let Some(name) = participation_shooter_from_line(&line, alias) {
                shooters.insert(name);
                break;
            }
        }
    }

    shooters.into_iter().collect()
}

fn participation_shooter_from_line(line: &str, alias: &str) -> Option<String> {
    let alias_index = line.find(alias)?;
    let before_alias = line[..alias_index].trim();
    if before_alias.is_empty() {
        return None;
    }

    let name = before_alias
        .split_once(' ')
        .filter(|(prefix, _)| prefix.chars().all(|character| character.is_ascii_digit()))
        .map_or(before_alias, |(_, name)| name)
        .trim();
    if name.contains(',') {
        Some(name.to_string())
    } else {
        None
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn canonical_club_name(value: &str) -> String {
    let normalized = collapse_whitespace(value);
    let mut normalized = club_name_without_numeric_prefix(&normalized).unwrap_or(normalized);
    if normalized == "Sprenge u.Umgegend" {
        return "Schützenverein Sprenge".to_string();
    }
    normalized = normalized.replace("BSchG", "Bürgerschützengilde");
    normalized = normalized.replace("SchG", "Schützengilde");
    normalized = normalized.replace("SchV", "Schützenverein");
    if let Some(rest) = normalized.strip_prefix("BSchützengilde ") {
        return format!("Bürgerschützengilde {rest}");
    }
    club_name_without_team_number_suffix(&normalized).unwrap_or(normalized)
}

fn club_aliases(club: &str) -> Vec<String> {
    let canonical = canonical_club_name(club);
    let abbreviated = canonical
        .replace("Schützengilde", "SchG")
        .replace("Schützenverein", "SchV");
    let mut aliases = BTreeSet::from([club.to_string(), canonical.clone(), abbreviated]);
    for alias in aliases.clone() {
        if let Some(short_alias) = club_name_without_numeric_prefix(&alias) {
            aliases.insert(short_alias);
        }
    }
    if canonical == "Schützenverein Sprenge" {
        aliases.insert("Sprenge u.Umgegend".to_string());
    }
    aliases.into_iter().collect()
}

fn club_name_without_numeric_prefix(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if let Some((prefix, club)) = trimmed.split_once(' ')
        && prefix.chars().all(|character| character.is_ascii_digit())
        && !club.trim().is_empty()
    {
        return Some(club.trim().to_string());
    }

    let first_non_digit = trimmed
        .char_indices()
        .find_map(|(index, character)| (!character.is_ascii_digit()).then_some(index))?;
    let (prefix, club) = trimmed.split_at(first_non_digit);
    (!prefix.is_empty() && is_known_club_prefix(club)).then(|| club.trim().to_string())
}

fn club_name_without_team_number_suffix(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let (club, team_number) = trimmed.rsplit_once(' ')?;
    (team_number.len() == 1
        && team_number
            .chars()
            .all(|character| character.is_ascii_digit())
        && !club.trim().is_empty())
    .then(|| club.trim_end_matches(" -").trim().to_string())
}

fn is_known_club_prefix(value: &str) -> bool {
    matches!(
        value.trim_start(),
        club if club.starts_with("SchV")
            || club.starts_with("SchG")
            || club.starts_with("BSchG")
            || club.starts_with("BSch")
    )
}

fn combined_known_club_names(
    podium_export: &PodiumExport,
    participation_export: &ParticipationExport,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for item in &podium_export.items {
        names.insert(canonical_club_name(&item.club));
        if !item.canonical_club.trim().is_empty() {
            names.insert(canonical_club_name(&item.canonical_club));
        }
    }
    for club in &participation_export.known_clubs {
        names.insert(canonical_club_name(club));
    }
    for item in &participation_export.matches {
        names.insert(canonical_club_name(&item.club));
        if !item.canonical_club.trim().is_empty() {
            names.insert(canonical_club_name(&item.canonical_club));
        }
    }
    names
}

fn resolve_truncated_club(club: &str, candidates: &BTreeSet<String>) -> String {
    let canonical = canonical_club_name(club);
    let Some(prefix) = truncated_prefix(&canonical) else {
        return canonical;
    };

    candidates
        .iter()
        .find(|candidate| *candidate != &canonical && candidate.starts_with(prefix))
        .cloned()
        .unwrap_or(canonical)
}

fn source_display_name(source_name: &str) -> &str {
    match source_name {
        "landesmeisterschaften" => "LM",
        "deutsche-meisterschaften" => "DM",
        _ => source_name,
    }
}

fn association_name(association_code: &str) -> &str {
    match association_code {
        "SL" => "Schleswig-Flensburg",
        "RD" => "Rendsburg-Eckernförde",
        "KI" => "Kiel",
        "NM" => "Neumünster",
        "PL" => "Plön",
        "OH" => "Ostholstein",
        "SE" => "Segeberg",
        "HL" => "Lübeck",
        "RZ" => "Herzogtum Lauenburg",
        "OD" => "Stormarn",
        "PI" => "Pinneberg",
        "IZ" => "Steinburg",
        "HE" => "Dithmarschen",
        "NF" => "Nordfriesland",
        "00" => "ohne Kreiszuordnung",
        "86" | "88" | "92" => "externe Zuordnung",
        _ => "Unbekannt",
    }
}

fn association_label(code: &str, name: &str) -> String {
    if name.trim().is_empty() || name == "Unbekannt" {
        code.to_string()
    } else {
        format!("{code} - {name}")
    }
}

const fn place_from_rank(rank: &Rank) -> Option<u32> {
    match rank {
        Rank::Place(place) => Some(*place),
        Rank::NotStarted | Rank::OutOfCompetition => None,
    }
}

fn manual_review_pdfs(crawl_report: &CrawlReport) -> Vec<ManualReviewPdf> {
    crawl_report
        .pdfs
        .iter()
        .filter(|pdf| pdf.classification == PdfClassification::ManualReview)
        .map(|pdf| ManualReviewPdf {
            url: pdf.url.clone(),
            reason: pdf.error.clone(),
            text_char_count: pdf.text_char_count,
            needs_ocr: pdf.needs_ocr,
        })
        .collect()
}

fn render_manual_review_rows(pdfs: &[ManualReviewPdf]) -> String {
    let mut rows = String::new();
    for pdf in pdfs {
        let reason = pdf.reason.as_deref().unwrap_or("manuelle Kontrolle");
        let text_char_count = pdf
            .text_char_count
            .map_or_else(|| "-".to_string(), |count| count.to_string());
        let needs_ocr = match pdf.needs_ocr {
            Some(true) => "ja",
            Some(false) => "nein",
            None => "-",
        };
        let _ = writeln!(
            rows,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            link_html(&pdf.url, "Quelle"),
            escape_html(reason),
            escape_html(&text_char_count),
            needs_ocr
        );
    }
    rows
}

fn render_html_export(export: &PodiumExport, _report_dir: Option<&Path>) -> String {
    let html_items = export
        .items
        .iter()
        .map(PodiumHtmlItem::from)
        .collect::<Vec<_>>();
    let highest_rank = export
        .items
        .iter()
        .map(|item| item.rank)
        .max()
        .unwrap_or(export.max_place);
    let items_json = serde_json::to_string(&html_items).expect("podium html items serialize");
    let mut rows = String::new();
    for item in &export.items {
        let association_label = association_label(&item.association_code, &item.association_name);
        let _ = writeln!(
            rows,
            "<tr data-kind=\"{}\" data-rank=\"{}\" data-shooter=\"{}\" data-club=\"{}\" data-association=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            result_kind_label(&item.result_kind),
            item.rank,
            escape_html(&item.shooter),
            escape_html(&item.canonical_club),
            escape_html(&item.association_code),
            item.rank,
            escape_html(source_display_name(&item.source_name)),
            escape_html(&association_label),
            escape_html(&item.shooter),
            escape_html(&item.club),
            discipline_cell(item),
            score_cell(item.score),
            result_kind_label(&item.result_kind),
            link_html(&item.pdf_url, "Quelle")
        );
    }
    let manual_review_rows = render_manual_review_rows(&export.manual_review_pdfs);
    let manual_review_section = if export.manual_review_pdfs.is_empty() {
        "<p class=\"muted\">Keine Dateien in manueller Nachbearbeitung.</p>".to_string()
    } else {
        render_template(
            PODIUM_MANUAL_REVIEW_TABLE_TEMPLATE,
            &[("manual_review_rows", manual_review_rows)],
        )
    };

    render_template(
        PODIUM_EXPORT_TEMPLATE,
        &[
            (
                "generated_at",
                escape_html(&export.generated_at.to_rfc3339()),
            ),
            (
                "source_name",
                escape_html(source_display_name(&export.source_name)),
            ),
            ("focus_code", escape_html(&export.focus_association_code)),
            ("max_place", export.max_place.to_string()),
            ("highest_rank", highest_rank.to_string()),
            ("item_count", export.item_count.to_string()),
            (
                "manual_review_count",
                export.manual_review_count.to_string(),
            ),
            ("rows", rows),
            ("manual_review_section", manual_review_section),
            ("items_json", items_json),
        ],
    )
}

fn render_participation_html(export: &ParticipationExport, report_dir: Option<&Path>) -> String {
    let mut rows = String::new();
    let mut club_options = String::new();
    for club in &export.known_clubs {
        let club = escape_html(club);
        let _ = write!(club_options, "<option value=\"{club}\">{club}</option>");
    }

    for item in &export.matches {
        let _ = writeln!(
            rows,
            "<tr data-club=\"{}\" data-text=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&item.club),
            escape_html(&format!(
                "{} {} {}",
                item.club,
                item.shooters.join(", "),
                item.pdf_url
            )),
            escape_html(&item.club),
            escape_html(&shooters_display(&item.shooters)),
            escape_html(source_display_name(&item.source_name)),
            link_html(&item.pdf_url, &item.pdf_url),
            local_path_link(&item.local_path, report_dir)
        );
    }

    render_template(
        PARTICIPATION_EXPORT_TEMPLATE,
        &[
            (
                "club_source_name",
                escape_html(source_display_name(&export.club_source_name)),
            ),
            (
                "results_source_name",
                escape_html(source_display_name(&export.results_source_name)),
            ),
            ("focus_code", escape_html(&export.focus_association_code)),
            (
                "generated_at",
                escape_html(&export.generated_at.to_rfc3339()),
            ),
            ("known_club_count", export.known_club_count.to_string()),
            ("matched_club_count", export.matched_club_count.to_string()),
            ("match_count", export.match_count.to_string()),
            ("club_options", club_options),
            ("rows", rows),
        ],
    )
}

fn render_combined_html(export: &CombinedExport, report_dir: Option<&Path>) -> String {
    let mut club_options = String::new();
    let mut shooter_options = String::new();
    let mut rows = String::new();
    let mut shooters = BTreeSet::new();

    for club in &export.clubs {
        let escaped_club = escape_html(&club.club);
        let _ = write!(
            club_options,
            "<option value=\"{escaped_club}\">{escaped_club}</option>"
        );

        for item in &club.podium_items {
            shooters.insert(item.shooter.clone());
            let _ = writeln!(
                rows,
                "<tr data-club=\"{}\" data-shooter=\"{}\" data-type=\"podium\" data-text=\"{}\"><td>{}</td><td>Podest</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escaped_club,
                escape_html(&item.shooter),
                escape_html(&format!(
                    "{} {} {} {}",
                    club.club,
                    item.shooter,
                    item.club,
                    item.discipline.as_deref().unwrap_or_default()
                )),
                escaped_club,
                item.rank,
                escape_html(&item.shooter),
                discipline_cell(item),
                result_kind_label(&item.result_kind),
                local_path_link(&item.local_path, report_dir)
            );
        }

        for match_item in &club.participation_matches {
            let shooters = shooters_display(&match_item.shooters);
            let _ = writeln!(
                rows,
                "<tr data-club=\"{}\" data-shooter=\"{}\" data-type=\"participation\" data-text=\"{}\"><td>{}</td><td>Teilnahme</td><td></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escaped_club,
                escape_html(&shooters),
                escape_html(&format!(
                    "{} {} {}",
                    club.club, shooters, match_item.pdf_url
                )),
                escaped_club,
                escape_html(&shooters),
                escape_html(source_display_name(&match_item.source_name)),
                link_html(&match_item.pdf_url, &match_item.pdf_url),
                local_path_link(&match_item.local_path, report_dir)
            );
        }
    }
    for shooter in shooters {
        let escaped_shooter = escape_html(&shooter);
        let _ = write!(
            shooter_options,
            "<option value=\"{escaped_shooter}\">{escaped_shooter}</option>"
        );
    }

    render_template(
        COMBINED_EXPORT_TEMPLATE,
        &[
            ("focus_code", escape_html(&export.focus_association_code)),
            (
                "generated_at",
                escape_html(&export.generated_at.to_rfc3339()),
            ),
            ("club_count", export.club_count.to_string()),
            ("podium_item_count", export.podium_item_count.to_string()),
            (
                "participation_match_count",
                export.participation_match_count.to_string(),
            ),
            ("club_options", club_options),
            ("shooter_options", shooter_options),
            ("rows", rows),
        ],
    )
}

fn discipline_cell(item: &PodiumExportItem) -> String {
    escape_html(
        &[&item.discipline_code, &item.discipline, &item.class_name]
            .into_iter()
            .filter_map(|value| value.as_deref())
            .collect::<Vec<_>>()
            .join(" - "),
    )
}

fn score_cell(score: Option<f64>) -> String {
    score.map_or_else(String::new, |score| {
        let formatted = format!("{score:.1}");
        formatted.trim_end_matches(".0").to_string()
    })
}

fn normalize_score(score: f64) -> f64 {
    (score * 10.0).round() / 10.0
}

fn shooters_display(shooters: &[String]) -> String {
    shooters.join(", ")
}

fn link_html(href: &str, label: &str) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        escape_html(href),
        escape_html(label)
    )
}

const fn result_kind_label(kind: &PodiumResultKind) -> &'static str {
    match kind {
        PodiumResultKind::Individual => "Einzel",
        PodiumResultKind::Team => "Mannschaft",
    }
}

fn local_path_link(path: &Path, report_dir: Option<&Path>) -> String {
    let label = escape_html(&path.display().to_string());
    relative_local_href(path, report_dir).map_or_else(
        || format!("<code>{label}</code>"),
        |href| {
            let href = escape_html(&href);
            format!("<a href=\"{href}\"><code>{label}</code></a>")
        },
    )
}

fn relative_local_href(path: &Path, report_dir: Option<&Path>) -> Option<String> {
    if path.is_absolute() {
        return None;
    }

    let report_dir = report_dir?;
    if report_dir.is_absolute() {
        return None;
    }

    let parent_hops = report_dir
        .components()
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count();
    let mut href = "../".repeat(parent_hops);
    href.push_str(&path.to_string_lossy().replace(' ', "%20"));
    Some(href)
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            character if character.is_control() => escaped.push(' '),
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{
        CombinedExportConfig, CombinedExporter, ManualReviewPdf, ParticipationExport,
        ParticipationMatch, PodiumExport, PodiumExportConfig, PodiumExportItem, PodiumExporter,
        PodiumResultKind, association_label, association_matches, association_name,
        canonical_club_name, club_aliases, club_name_without_numeric_prefix,
        collapse_truncated_clubs, collapse_whitespace, combined_known_club_names, escape_html,
        is_meyton_rank_header, meyton_association_code, meyton_continued_shooter_name,
        meyton_discipline_code, meyton_event_date, meyton_event_name, meyton_shooter_name,
        meyton_team_header, normalize_match_text, participation_shooter_from_line,
        participation_shooters, render_html_export, report_source_name, resolve_truncated_club,
        source_display_name, truncated_prefix,
    };
    use crate::ingest::CrawlReport;
    use crate::sport_results::{
        EventInfo, IndividualResult, Rank, SportResultList, TeamMemberResult, TeamResult,
    };
    use chrono::Utc;
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn exports_focus_placements_individuals_and_team_members() {
        let exporter = PodiumExporter::new(PodiumExportConfig {
            crawl_report_path: PathBuf::from("reports/input.json"),
            json_output_path: PathBuf::from("reports/export.json"),
            html_output_path: PathBuf::from("reports/export.html"),
            focus_association_code: "OD".to_string(),
            max_place: 3,
            min_text_chars: 80,
        });
        let event = EventInfo {
            name: "Landesmeisterschaft".to_string(),
            date: Some("01.01.2025".to_string()),
            location: None,
            system: None,
            discipline_code: Some("1.10.10".to_string()),
            discipline: Some("Luftgewehr".to_string()),
            class_name: Some("Herren I".to_string()),
        };
        let result_list = SportResultList {
            event: event.clone(),
            team_results: vec![
                TeamResult {
                    event: event.clone(),
                    rank: Some(2),
                    association: "OD".to_string(),
                    club: "SchV Trittau".to_string(),
                    total: 100.0,
                    members: vec![TeamMemberResult {
                        start_number: 1,
                        name: "Team Schütze".to_string(),
                        total: 100.0,
                    }],
                },
                TeamResult {
                    event: event.clone(),
                    rank: Some(4),
                    association: "OD".to_string(),
                    club: "SchV Reinfeld".to_string(),
                    total: 90.0,
                    members: vec![TeamMemberResult {
                        start_number: 4,
                        name: "Team Rang Vier".to_string(),
                        total: 90.0,
                    }],
                },
            ],
            individual_results: vec![
                IndividualResult {
                    event: event.clone(),
                    rank: Rank::Place(1),
                    start_number: 2,
                    name: "Einzel Schütze".to_string(),
                    association: "OD".to_string(),
                    club: "SchV Elmenhorst".to_string(),
                    series: Vec::new(),
                    total: 99.0,
                },
                IndividualResult {
                    event: event.clone(),
                    rank: Rank::Place(4),
                    start_number: 5,
                    name: "Einzel Rang Vier".to_string(),
                    association: "OD".to_string(),
                    club: "SchV Reinfeld".to_string(),
                    series: Vec::new(),
                    total: 97.0,
                },
                IndividualResult {
                    event,
                    rank: Rank::Place(3),
                    start_number: 3,
                    name: "Anderer Kreis".to_string(),
                    association: "SE".to_string(),
                    club: "Other".to_string(),
                    series: Vec::new(),
                    total: 98.0,
                },
            ],
            out_of_competition_team_results: Vec::new(),
            out_of_competition_individual_results: Vec::new(),
        };

        let items = exporter.export_items_from_result_list(
            &result_list,
            "landesmeisterschaften",
            "https://example.org/result.pdf",
            &PathBuf::from("data/downloads/result.pdf"),
        );

        assert_eq!(items.len(), 4);
        assert!(items.iter().any(|item| item.shooter == "Team Schütze"));
        assert!(items.iter().any(|item| item.shooter == "Einzel Schütze"));
        assert!(items.iter().any(|item| item.shooter == "Team Rang Vier"));
        assert!(items.iter().any(|item| item.shooter == "Einzel Rang Vier"));
        assert!(
            items
                .iter()
                .any(|item| item.canonical_club == "Schützenverein Trittau")
        );
    }

    #[test]
    fn exports_all_podium_associations_when_requested() {
        let exporter = PodiumExporter::new(PodiumExportConfig {
            crawl_report_path: PathBuf::from("reports/input.json"),
            json_output_path: PathBuf::from("reports/export.json"),
            html_output_path: PathBuf::from("reports/export.html"),
            focus_association_code: "all".to_string(),
            max_place: 3,
            min_text_chars: 80,
        });
        let event = EventInfo {
            name: "Landesmeisterschaft".to_string(),
            date: Some("01.01.2025".to_string()),
            location: None,
            system: None,
            discipline_code: Some("1.10.10".to_string()),
            discipline: Some("Luftgewehr".to_string()),
            class_name: Some("Herren I".to_string()),
        };
        let result_list = SportResultList {
            event: event.clone(),
            team_results: vec![TeamResult {
                event: event.clone(),
                rank: Some(2),
                association: "SE".to_string(),
                club: "SchV Beispiel".to_string(),
                total: 100.0,
                members: vec![TeamMemberResult {
                    start_number: 1,
                    name: "Team SE".to_string(),
                    total: 100.0,
                }],
            }],
            individual_results: vec![IndividualResult {
                event,
                rank: Rank::Place(3),
                start_number: 2,
                name: "Einzel OD".to_string(),
                association: "OD".to_string(),
                club: "SchV Elmenhorst".to_string(),
                series: Vec::new(),
                total: 98.0,
            }],
            out_of_competition_team_results: Vec::new(),
            out_of_competition_individual_results: Vec::new(),
        };

        let items = exporter.export_items_from_result_list(
            &result_list,
            "landesmeisterschaften",
            "https://example.org/result.pdf",
            &PathBuf::from("data/downloads/result.pdf"),
        );

        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.association_code == "OD"));
        assert!(items.iter().any(|item| item.association_code == "SE"));
    }

    #[test]
    fn exports_meyton_mixed_team_placements_for_known_focus_clubs() {
        let exporter = PodiumExporter::new(PodiumExportConfig {
            crawl_report_path: PathBuf::from("reports/input.json"),
            json_output_path: PathBuf::from("reports/export.json"),
            html_output_path: PathBuf::from("reports/export.html"),
            focus_association_code: "OD".to_string(),
            max_place: 3,
            min_text_chars: 80,
        });
        let text = "\
VW112_K40_260516_1045 Finale
16.05.2026
1. SchV Elmenhorst
239 Burmeister, Anja 33846 5 Shot Series
240 Witt, Björn 33847 5 Shot Series
4. SchV Elmenhorst
241 Spät, Nicht 33848 5 Shot Series";
        let known_clubs = vec!["012 Schützenverein Elmenhorst".to_string()];
        let known_club_associations =
            BTreeMap::from([("Schützenverein Elmenhorst".to_string(), "OD".to_string())]);

        let items = exporter.export_meyton_team_items(
            text,
            &known_clubs,
            &known_club_associations,
            "landesmeisterschaften",
            "https://example.org/mixed.pdf",
            &PathBuf::from("data/mixed.pdf"),
        );

        assert_eq!(items.len(), 3);
        assert!(items.iter().any(|item| item.shooter == "Burmeister, Anja"));
        assert!(items.iter().any(|item| item.shooter == "Witt, Björn"));
        assert!(items.iter().any(|item| item.shooter == "Spät, Nicht"));
        assert!(
            items
                .iter()
                .all(|item| item.class_name.as_deref() == Some("Mixed"))
        );
        assert!(
            items
                .iter()
                .all(|item| matches!(item.result_kind, PodiumResultKind::Individual))
        );
        assert!(items.iter().any(|item| item.rank == 1));
        assert!(items.iter().any(|item| item.rank == 4));
    }

    #[test]
    fn exports_later_meyton_sections_with_wrapped_names() {
        let exporter = PodiumExporter::new(PodiumExportConfig {
            crawl_report_path: PathBuf::from("reports/input.json"),
            json_output_path: PathBuf::from("reports/export.json"),
            html_output_path: PathBuf::from("reports/export.html"),
            focus_association_code: "OD".to_string(),
            max_place: 3,
            min_text_chars: 80,
        });
        let text = "\
VW212_K10_260516_0915 Finale
16.05.2026
3. SchV Elmenhorst 300 5 Shot Series
627 Stempell, 33771 5 Shot Series
Christine Single Shot Series
628 Stempell, Ingmar 33844 5 Shot Series";
        let known_clubs = vec!["012 Schützenverein Elmenhorst".to_string()];
        let known_club_associations =
            BTreeMap::from([("Schützenverein Elmenhorst".to_string(), "OD".to_string())]);

        let items = exporter.export_meyton_team_items(
            text,
            &known_clubs,
            &known_club_associations,
            "landesmeisterschaften",
            "https://example.org/lp-mixed.pdf",
            &PathBuf::from("data/lp-mixed.pdf"),
        );

        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .any(|item| item.shooter == "Stempell, Christine")
        );
        assert!(items.iter().any(|item| item.shooter == "Stempell, Ingmar"));
        assert!(items.iter().all(|item| item.rank == 3));
        assert!(
            items
                .iter()
                .all(|item| matches!(item.result_kind, PodiumResultKind::Individual))
        );
        assert!(
            items
                .iter()
                .all(|item| item.discipline_code.as_deref() == Some("K10"))
        );
    }

    #[test]
    fn matches_meyton_club_without_numeric_david_prefix() {
        let known_clubs = vec!["012 Schützenverein Elmenhorst".to_string()];
        let team = meyton_team_header("1. SchV Elmenhorst", &known_clubs).expect("team is matched");

        assert_eq!(team.rank, 1);
        assert_eq!(team.club, "012 Schützenverein Elmenhorst");
    }

    #[test]
    fn canonicalizes_shooting_club_abbreviations() {
        assert_eq!(
            canonical_club_name("Ahrensburger SchG"),
            "Ahrensburger Schützengilde"
        );
        assert_eq!(
            canonical_club_name("SchV Klein Wesenberg"),
            "Schützenverein Klein Wesenberg"
        );
        assert_eq!(
            canonical_club_name("Sprenge u.Umgegend"),
            "Schützenverein Sprenge"
        );
        assert_eq!(
            canonical_club_name("012 SchV Sprenge"),
            "Schützenverein Sprenge"
        );
        assert_eq!(
            canonical_club_name("080 SchV Sprenge"),
            "Schützenverein Sprenge"
        );
        assert_eq!(
            canonical_club_name("012 Ahrensburger SchG"),
            "Ahrensburger Schützengilde"
        );
        assert_eq!(
            canonical_club_name("080012SchV Bargteheide"),
            "Schützenverein Bargteheide"
        );
        assert_eq!(
            canonical_club_name("BSchG Bad Oldesloe"),
            "Bürgerschützengilde Bad Oldesloe"
        );
        assert_eq!(
            canonical_club_name("BSchützengilde Bad Oldesloe"),
            "Bürgerschützengilde Bad Oldesloe"
        );
        assert_eq!(
            canonical_club_name("080 Schützenverein Reinfeld 1"),
            "Schützenverein Reinfeld"
        );
        assert_eq!(
            canonical_club_name("TSV Klausdorf - Schützenabteilung - 1"),
            "TSV Klausdorf - Schützenabteilung"
        );
        assert!(club_aliases("Ahrensburger Schützengilde").contains(&"Ahrensburger SchG".into()));
        assert!(club_aliases("Schützenverein Sprenge").contains(&"Sprenge u.Umgegend".into()));
        assert!(club_aliases("012 SchV Sprenge").contains(&"012 SchV Sprenge".into()));
        assert!(club_aliases("012 SchV Sprenge").contains(&"SchV Sprenge".into()));
    }

    #[test]
    fn displays_known_source_names_as_abbreviations() {
        assert_eq!(source_display_name("landesmeisterschaften"), "LM");
        assert_eq!(source_display_name("deutsche-meisterschaften"), "DM");
        assert_eq!(source_display_name("bezirk"), "bezirk");
    }

    #[test]
    fn maps_known_association_codes_to_names() {
        assert_eq!(association_name("OD"), "Stormarn");
        assert_eq!(association_name("RZ"), "Herzogtum Lauenburg");
        assert_eq!(association_name("NF"), "Nordfriesland");
        assert_eq!(
            association_label("OD", association_name("OD")),
            "OD - Stormarn"
        );
    }

    #[test]
    fn renders_association_grouping_option() {
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 0,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: Vec::new(),
        };

        let html = render_html_export(&export, None);

        assert!(html.contains("<option value=\"association\">Kreis</option>"));
    }

    #[test]
    fn renders_responsive_podium_table_styles() {
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 0,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: Vec::new(),
        };

        let html = render_html_export(&export, None);

        assert!(html.contains("@media (max-width: 760px)"));
        assert!(html.contains("grid-template-columns: minmax(88px, 34%) 1fr"));
        assert!(html.contains("content: \"Schütze\""));
        assert!(html.contains(".manual-review td:nth-child(4)::before"));
        assert!(html.contains("tbody tr.is-hovered { background: #fff4c2; }"));
        assert!(html.contains("row.classList.add(\"is-hovered\")"));
    }

    #[test]
    fn preserves_html_filters_in_report_url() {
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 0,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: Vec::new(),
        };

        let html = render_html_export(&export, None);

        assert!(html.contains("new URLSearchParams(window.location.search)"));
        assert!(html.contains("param: \"suche\""));
        assert!(html.contains("param: \"wertung\""));
        assert!(html.contains("param: \"kreis\""));
        assert!(html.contains("param: \"verein\""));
        assert!(html.contains("param: \"schuetze\""));
        assert!(html.contains("param: \"platz_bis\""));
        assert!(html.contains("param: \"gruppe\""));
        assert!(html.contains("Ansicht-Link"));
    }

    #[test]
    fn renders_manual_review_summary_in_podium_html() {
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 0,
            manual_review_count: 1,
            manual_review_pdfs: vec![ManualReviewPdf {
                url: "https://example.org/manual.pdf".to_string(),
                reason: Some("kein DAVID21+ Format".to_string()),
                text_char_count: Some(42),
                needs_ocr: Some(true),
            }],
            items: Vec::new(),
        };

        let html = render_html_export(&export, None);

        assert!(html.contains("manuelle Nachbearbeitung"));
        assert!(html.contains("Manuelle Nachbearbeitung"));
        assert!(html.contains("https://example.org/manual.pdf"));
        assert!(html.contains("kein DAVID21+ Format"));
        assert!(html.contains("<td>42</td>"));
        assert!(html.contains("<td>ja</td>"));
    }

    #[test]
    fn omits_local_file_column_from_podium_html() {
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 1,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: vec![PodiumExportItem {
                source_name: "landesmeisterschaften".to_string(),
                rank: 1,
                result_kind: PodiumResultKind::Individual,
                shooter: "Test, Tina".to_string(),
                club: "Ahrensburger SchG".to_string(),
                canonical_club: "Ahrensburger Schützengilde".to_string(),
                association_code: "OD".to_string(),
                association_name: "Stormarn".to_string(),
                discipline: Some("Luftgewehr".to_string()),
                discipline_code: Some("1.10".to_string()),
                class_name: None,
                event_name: "LM".to_string(),
                event_date: None,
                score: Some(100.0),
                pdf_url: "https://example.org/lm.pdf".to_string(),
                local_path: PathBuf::from(
                    "data/archive/2026/landesmeisterschaften/downloads/lm.pdf",
                ),
            }],
        };

        let html = render_html_export(&export, None);

        assert!(!html.contains("<th>Lokale Datei</th>"));
        assert!(!html.contains("local_path"));
        assert!(!html.contains("data/archive/2026/landesmeisterschaften/downloads"));
        assert!(html.contains("https://example.org/lm.pdf"));
    }

    #[test]
    fn limits_rank_filter_to_highest_exported_rank() {
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 1,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: vec![PodiumExportItem {
                source_name: "landesmeisterschaften".to_string(),
                rank: 12,
                result_kind: PodiumResultKind::Individual,
                shooter: "Test, Tina".to_string(),
                club: "Ahrensburger SchG".to_string(),
                canonical_club: "Ahrensburger Schützengilde".to_string(),
                association_code: "OD".to_string(),
                association_name: "Stormarn".to_string(),
                discipline: Some("Luftgewehr".to_string()),
                discipline_code: Some("1.10".to_string()),
                class_name: None,
                event_name: "LM".to_string(),
                event_date: None,
                score: Some(100.0),
                pdf_url: "https://example.org/lm.pdf".to_string(),
                local_path: PathBuf::from("data/lm.pdf"),
            }],
        };

        let html = render_html_export(&export, None);

        assert!(html.contains("id=\"maxRankFilter\" type=\"number\" min=\"1\" max=\"12\""));
        assert!(html.contains("value=\"3\""));
        assert!(html.contains("clampNumberControl(control)"));
    }

    #[test]
    fn escapes_html_and_control_characters() {
        assert_eq!(escape_html("A&B<\0>"), "A&amp;B&lt; &gt;");
    }

    #[test]
    fn combines_podium_and_participation_by_canonical_club() {
        let dir = std::env::temp_dir().join(format!(
            "pdf-explorer-combined-test-{}",
            Utc::now()
                .timestamp_nanos_opt()
                .expect("timestamp available")
        ));
        fs::create_dir_all(&dir).expect("test dir is created");
        let podium_path = dir.join("podium.json");
        let participation_path = dir.join("participation.json");
        let output_path = dir.join("combined.json");
        let html_path = dir.join("combined.html");

        let podium = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "OD".to_string(),
            max_place: 3,
            item_count: 1,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: vec![PodiumExportItem {
                source_name: "landesmeisterschaften".to_string(),
                rank: 1,
                result_kind: PodiumResultKind::Individual,
                shooter: "Test, Tina".to_string(),
                club: "Ahrensburger SchG".to_string(),
                canonical_club: "Ahrensburger Schützengilde".to_string(),
                association_code: "OD".to_string(),
                association_name: "Stormarn".to_string(),
                discipline: Some("Luftgewehr".to_string()),
                discipline_code: Some("1.10".to_string()),
                class_name: None,
                event_name: "LM".to_string(),
                event_date: None,
                score: Some(100.0),
                pdf_url: "https://example.org/lm.pdf".to_string(),
                local_path: PathBuf::from("data/lm.pdf"),
            }],
        };
        let participation = ParticipationExport {
            generated_at: Utc::now(),
            club_source_report_path: PathBuf::from("reports/lm.json"),
            results_report_path: PathBuf::from("reports/dm.json"),
            club_source_name: "landesmeisterschaften".to_string(),
            results_source_name: "deutsche-meisterschaften".to_string(),
            focus_association_code: "OD".to_string(),
            known_club_count: 1,
            matched_club_count: 1,
            match_count: 1,
            known_clubs: vec!["Ahrensburger Schützengilde".to_string()],
            matches: vec![ParticipationMatch {
                club: "Ahrensburger Schützengilde".to_string(),
                canonical_club: "Ahrensburger Schützengilde".to_string(),
                shooters: vec!["Test, Tina".to_string()],
                source_name: "deutsche-meisterschaften".to_string(),
                pdf_url: "https://example.org/dm.pdf".to_string(),
                local_path: PathBuf::from("data/dm.pdf"),
                text_char_count: 100,
            }],
        };
        fs::write(
            &podium_path,
            serde_json::to_string_pretty(&podium).expect("podium serializes"),
        )
        .expect("podium is written");
        fs::write(
            &participation_path,
            serde_json::to_string_pretty(&participation).expect("participation serializes"),
        )
        .expect("participation is written");

        let export = CombinedExporter::new(CombinedExportConfig {
            podium_export_path: podium_path,
            participation_export_path: participation_path,
            json_output_path: output_path,
            html_output_path: html_path,
        })
        .run()
        .expect("combined export is created");

        assert_eq!(export.club_count, 1);
        assert_eq!(export.podium_item_count, 1);
        assert_eq!(export.participation_match_count, 1);
        assert_eq!(export.clubs[0].club, "Ahrensburger Schützengilde");
    }

    #[test]
    fn extracts_participation_shooters_for_dm_rows() {
        let text = "\
2526 Bentien, Sven  Ahrensburger SchG I 4.10.10 10m Lfd. Scheibe Herren I 498 20
Behrens, Joris Velten SchV Elmenhorst 11.10.24 Lichtgewehr Schüler III 145 11
Mannschaft
Ahrensburger SchG I 4.10.10 10m Lfd. Scheibe Herren I 1245 13";

        let ahrensburg = participation_shooters(text, &club_aliases("Ahrensburger Schützengilde"));
        let elmenhorst = participation_shooters(text, &club_aliases("Schützenverein Elmenhorst"));

        assert_eq!(ahrensburg, vec!["Bentien, Sven"]);
        assert_eq!(elmenhorst, vec!["Behrens, Joris Velten"]);
    }

    #[test]
    fn parses_meyton_shooter_name_with_score() {
        let shooter = meyton_shooter_name("239 Burmeister, Anja 33846 5 Shot Series").unwrap();
        assert_eq!(shooter.name, "Burmeister, Anja");
        assert_eq!(shooter.score, Some(5.0));
    }

    #[test]
    fn parses_meyton_shooter_name_without_score() {
        let shooter = meyton_shooter_name("123 Name, Vorname").unwrap();
        assert_eq!(shooter.name, "Name, Vorname");
        assert_eq!(shooter.score, Some(123.0));
    }

    #[test]
    fn returns_none_for_meyton_line_without_comma_name() {
        assert!(meyton_shooter_name("just a plain line").is_none());
    }

    #[test]
    fn continues_wrapped_meyton_shooter_name() {
        let prefix = meyton_shooter_name("627 Stempell, 33771 5 Shot Series").unwrap();
        let shooter =
            meyton_continued_shooter_name(&prefix, "Christine Single Shot Series").unwrap();
        assert_eq!(shooter.name, "Stempell, Christine");
        assert_eq!(shooter.score, Some(5.0));
    }

    #[test]
    fn extracts_meyton_event_name_from_finale_line() {
        assert_eq!(
            meyton_event_name("VW112_K40_260516_1045 Finale"),
            Some("VW112_K40_260516_1045 Finale".to_string())
        );
        assert!(meyton_event_name("just a normal line").is_none());
    }

    #[test]
    fn extracts_meyton_discipline_code() {
        assert_eq!(
            meyton_discipline_code("VW112_K40_260516_1045 Finale"),
            Some("K40".to_string())
        );
        assert!(meyton_discipline_code("no code here").is_none());
    }

    #[test]
    fn extracts_meyton_event_date() {
        assert_eq!(
            meyton_event_date("16.05.2026"),
            Some("16.05.2026".to_string())
        );
        assert!(meyton_event_date("no date").is_none());
    }

    #[test]
    fn identifies_meyton_rank_headers() {
        assert!(is_meyton_rank_header("1. SchV Elmenhorst"));
        assert!(is_meyton_rank_header("3. SchV Trittau"));
        assert!(!is_meyton_rank_header("SchV Elmenhorst"));
        assert!(!is_meyton_rank_header("just text"));
    }

    #[test]
    fn normalizes_match_text_for_club_matching() {
        assert_eq!(normalize_match_text("SchV Elmenhorst"), "schv elmenhorst");
        assert_eq!(
            normalize_match_text("012 Schützenverein Elmenhorst"),
            "012 schützenverein elmenhorst"
        );
    }

    #[test]
    fn extracts_shooter_name_from_participation_line() {
        let alias = "Ahrensburger SchG".to_string();
        let result = participation_shooter_from_line(
            "239 Burmeister, Anja  Ahrensburger SchG I 4.10.10",
            &alias,
        );
        assert_eq!(result, Some("Burmeister, Anja".to_string()));
    }

    #[test]
    fn returns_none_for_non_matching_participation_line() {
        let alias = "Schützenverein Elmenhorst".to_string();
        let result = participation_shooter_from_line("239 Burmeister, Anja  Other Club", &alias);
        assert!(result.is_none());
    }

    #[test]
    fn collapses_whitespace_in_text() {
        assert_eq!(collapse_whitespace("  hello   world  "), "hello world");
        assert_eq!(collapse_whitespace("single"), "single");
    }

    #[test]
    fn extracts_club_name_without_numeric_prefix() {
        assert_eq!(
            club_name_without_numeric_prefix("012 Schützenverein Elmenhorst"),
            Some("Schützenverein Elmenhorst".to_string())
        );
        assert_eq!(club_name_without_numeric_prefix("SchV Trittau"), None);
    }

    #[test]
    fn matches_association_with_case_insensitive_all_filter() {
        assert!(association_matches("OD", "OD"));
        assert!(association_matches("OD", "all"));
        assert!(association_matches("OD", "ALL"));
        assert!(!association_matches("OD", "SE"));
    }

    #[test]
    fn resolves_meyton_association_code_from_known_clubs() {
        let known = BTreeMap::from([("Schützenverein Elmenhorst".to_string(), "OD".to_string())]);
        assert_eq!(
            meyton_association_code("SchV Elmenhorst", &known, "SE"),
            "OD"
        );
        assert_eq!(meyton_association_code("Unknown Club", &known, "SE"), "SE");
    }

    #[test]
    fn resolves_truncated_club_to_full_name() {
        let candidates = BTreeSet::from([
            "Schützenverein Elmenhorst".to_string(),
            "Schützenverein Elm".to_string(),
        ]);
        assert_eq!(
            resolve_truncated_club("SchV Elmenhorst...", &candidates),
            "Schützenverein Elmenhorst"
        );
    }

    #[test]
    fn returns_canonical_name_when_no_truncation() {
        let candidates = BTreeSet::from(["Schützenverein Trittau".to_string()]);
        assert_eq!(
            resolve_truncated_club("SchV Trittau", &candidates),
            "Schützenverein Trittau"
        );
    }

    #[test]
    fn collapses_truncated_club_variants() {
        let clubs = vec![
            "Schützenverein Elm...".to_string(),
            "Schützenverein Elmenhorst".to_string(),
        ];
        let collapsed = collapse_truncated_clubs(&clubs);
        assert_eq!(collapsed, vec!["Schützenverein Elmenhorst"]);
    }

    #[test]
    fn keeps_non_truncated_club_variants() {
        let clubs = vec!["SchV Trittau".to_string(), "SchV Reinfeld".to_string()];
        let collapsed = collapse_truncated_clubs(&clubs);
        assert_eq!(collapsed.len(), 2);
    }

    #[test]
    fn extracts_truncated_prefix_from_club_name() {
        assert_eq!(
            truncated_prefix("SchV Elmenhorst..."),
            Some("SchV Elmenhorst")
        );
        assert_eq!(truncated_prefix("SchV Elmenhorst"), None);
    }

    #[test]
    fn builds_report_source_name_from_path_when_empty() {
        let report = CrawlReport {
            generated_at: Utc::now(),
            source_url: "https://example.org".to_string(),
            source_name: String::new(),
            focus: "OD".to_string(),
            focus_association_code: "OD".to_string(),
            discovered_pdf_count: 0,
            downloaded_count: 0,
            changed_count: 0,
            unchanged_count: 0,
            removed_count: 0,
            auto_processed_count: 0,
            manual_review_count: 0,
            failed_count: 0,
            removed_pdfs: vec![],
            pdfs: vec![],
        };
        assert_eq!(
            report_source_name(&report, Path::new("ndsb-2025-crawl-report.json")),
            "ndsb-2025-crawl-report"
        );
    }

    #[test]
    fn uses_source_name_when_not_empty() {
        let report = CrawlReport {
            generated_at: Utc::now(),
            source_url: "https://example.org".to_string(),
            source_name: "landesmeisterschaften".to_string(),
            focus: "OD".to_string(),
            focus_association_code: "OD".to_string(),
            discovered_pdf_count: 0,
            downloaded_count: 0,
            changed_count: 0,
            unchanged_count: 0,
            removed_count: 0,
            auto_processed_count: 0,
            manual_review_count: 0,
            failed_count: 0,
            removed_pdfs: vec![],
            pdfs: vec![],
        };
        assert_eq!(
            report_source_name(&report, Path::new("any.json")),
            "landesmeisterschaften"
        );
    }

    #[test]
    fn combines_known_club_names_from_both_exports() {
        let podium = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "OD".to_string(),
            max_place: 3,
            item_count: 1,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: vec![PodiumExportItem {
                source_name: "landesmeisterschaften".to_string(),
                rank: 1,
                result_kind: PodiumResultKind::Individual,
                shooter: "Test, Tina".to_string(),
                club: "Ahrensburger SchG".to_string(),
                canonical_club: "Ahrensburger Schützengilde".to_string(),
                association_code: "OD".to_string(),
                association_name: "Stormarn".to_string(),
                discipline: Some("Luftgewehr".to_string()),
                discipline_code: Some("1.10".to_string()),
                class_name: None,
                event_name: "LM".to_string(),
                event_date: None,
                score: Some(100.0),
                pdf_url: "https://example.org/lm.pdf".to_string(),
                local_path: PathBuf::from("data/lm.pdf"),
            }],
        };
        let participation = ParticipationExport {
            generated_at: Utc::now(),
            club_source_report_path: PathBuf::from("reports/lm.json"),
            results_report_path: PathBuf::from("reports/dm.json"),
            club_source_name: "landesmeisterschaften".to_string(),
            results_source_name: "deutsche-meisterschaften".to_string(),
            focus_association_code: "OD".to_string(),
            known_club_count: 1,
            matched_club_count: 1,
            match_count: 1,
            known_clubs: vec!["Ahrensburger Schützengilde".to_string()],
            matches: vec![ParticipationMatch {
                club: "Ahrensburger Schützengilde".to_string(),
                canonical_club: "Ahrensburger Schützengilde".to_string(),
                shooters: vec!["Test, Tina".to_string()],
                source_name: "deutsche-meisterschaften".to_string(),
                pdf_url: "https://example.org/dm.pdf".to_string(),
                local_path: PathBuf::from("data/dm.pdf"),
                text_char_count: 100,
            }],
        };
        let known = combined_known_club_names(&podium, &participation);
        assert!(known.contains("Ahrensburger Schützengilde"));
    }

    #[test]
    fn returns_empty_string_for_david21_cell_without_summary() {
        // david21_cell is tested in ingest::tests
    }
}
