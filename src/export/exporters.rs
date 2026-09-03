use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::ingest::{CrawlReport, PdfClassification};
use crate::pdf::{ExtractOptions, PdfExtractor};
use crate::sport_results::{SportResultList, SportResultsParser};

use super::html::{
    manual_review_pdfs, render_combined_html, render_html_export, render_participation_html,
};
use super::matching::{
    MeytonEvent, MeytonShooter, MeytonTeam, association_matches, association_name,
    canonical_club_name, collapse_whitespace, combined_known_club_names, is_meyton_rank_header,
    is_meyton_text, known_club_associations, known_focus_clubs, meyton_association_code,
    meyton_continued_shooter_name, meyton_discipline_code, meyton_event_date, meyton_event_name,
    meyton_export_text, meyton_shooter_name, meyton_team_header, participation_matches,
    place_from_rank, report_source_name, resolve_truncated_club,
};

use super::models::{
    CombinedClub, CombinedExport, CombinedExportConfig, ManualNameOverrides, ParticipationExport,
    ParticipationExportConfig, PodiumExport, PodiumExportConfig, PodiumExportItem,
    PodiumResultKind,
};

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

        apply_manual_overrides(&mut items, &self.config.manual_overrides);

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

    pub(super) fn export_items_from_result_list(
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

    pub(super) fn export_meyton_team_items(
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

pub(super) fn apply_manual_overrides(
    items: &mut [PodiumExportItem],
    overrides: &ManualNameOverrides,
) {
    for item in items {
        item.shooter = overrides.corrected_athlete_name(&item.shooter);
        item.canonical_club = overrides.corrected_club_name(&item.club, &item.canonical_club);
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

fn normalize_score(score: f64) -> f64 {
    (score * 10.0).round() / 10.0
}
