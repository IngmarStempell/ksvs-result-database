use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::ingest::{CrawlReport, PdfClassification};
use crate::pdf::PdfExtractor;
use crate::sport_results::{Rank, SportResultsParser};

use super::models::ParticipationMatch;

pub(super) fn known_focus_clubs(
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

pub(super) fn report_source_name(report: &CrawlReport, path: &Path) -> String {
    if report.source_name.trim().is_empty() {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        report.source_name.clone()
    }
}

pub(super) fn known_club_associations(
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

pub(super) fn association_matches(association: &str, filter: &str) -> bool {
    filter.eq_ignore_ascii_case("all") || association == filter
}

pub(super) fn meyton_association_code(
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
pub(super) struct MeytonTeam {
    pub(super) rank: u32,
    pub(super) club: String,
    pub(super) member_count: usize,
}

#[derive(Debug)]
pub(super) struct MeytonShooter {
    pub(super) name: String,
    pub(super) score: Option<f64>,
}

#[derive(Debug, Default)]
pub(super) struct MeytonEvent {
    pub(super) event_name: String,
    pub(super) event_date: Option<String>,
    pub(super) discipline_code: Option<String>,
}

pub(super) fn meyton_export_text(default_text: &str, local_path: &Path) -> String {
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

pub(super) fn is_meyton_text(text: &str) -> bool {
    text.contains("Meyton") || text.contains("Ranklist") || text.contains("Finale")
}

pub(super) fn meyton_event_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.contains("_K") && line.contains("Finale") {
        Some(line.to_string())
    } else {
        None
    }
}

pub(super) fn meyton_discipline_code(text: &str) -> Option<String> {
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

pub(super) fn meyton_event_date(text: &str) -> Option<String> {
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

pub(super) fn meyton_team_header(line: &str, known_clubs: &[String]) -> Option<MeytonTeam> {
    let (rank, club) = line.split_once(". ")?;
    let rank = rank.parse::<u32>().ok()?;
    let club = match_known_club(club, known_clubs)?;

    Some(MeytonTeam {
        rank,
        club,
        member_count: 0,
    })
}

pub(super) fn is_meyton_rank_header(line: &str) -> bool {
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

pub(super) fn meyton_shooter_name(line: &str) -> Option<MeytonShooter> {
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

pub(super) fn meyton_continued_shooter_name(
    prefix: &MeytonShooter,
    line: &str,
) -> Option<MeytonShooter> {
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

pub(super) fn collapse_truncated_clubs(clubs: &[String]) -> Vec<String> {
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

pub(super) fn truncated_prefix(club: &str) -> Option<&str> {
    club.find("...").map(|index| club[..index].trim_end())
}

pub(super) fn participation_matches(
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

pub(super) fn normalize_match_text(value: &str) -> String {
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

pub(super) fn participation_shooters(text: &str, aliases: &[String]) -> Vec<String> {
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

pub(super) fn participation_shooter_from_line(line: &str, alias: &str) -> Option<String> {
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

pub(super) fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn canonical_club_name(value: &str) -> String {
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

pub(super) fn club_aliases(club: &str) -> Vec<String> {
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

pub(super) fn club_name_without_numeric_prefix(value: &str) -> Option<String> {
    let stripped = value.trim_start_matches(|character: char| character.is_ascii_digit());
    if stripped == value {
        return None;
    }
    let stripped = stripped.trim_start_matches(|character: char| {
        character.is_ascii_whitespace() || character == '.' || character == '-'
    });
    if is_known_club_prefix(stripped) {
        Some(stripped.to_string())
    } else {
        None
    }
}

fn club_name_without_team_number_suffix(value: &str) -> Option<String> {
    let (name, suffix) = value.rsplit_once(' ')?;
    let suffix = suffix.trim_matches('.');
    let is_numeric = suffix.chars().all(|character| character.is_ascii_digit());
    let is_roman = matches!(
        suffix,
        "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X"
    );
    (is_numeric || is_roman).then(|| name.trim_end_matches(" -").trim().to_string())
}

fn is_known_club_prefix(value: &str) -> bool {
    value.starts_with("Sch")
        || value.starts_with("Ahrensburger")
        || value.starts_with("BSch")
        || value.starts_with("Bürgersch")
        || value.starts_with("TSV")
}

pub(super) fn combined_known_club_names(
    podium_export: &super::models::PodiumExport,
    participation_export: &super::models::ParticipationExport,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for club in &participation_export.known_clubs {
        names.insert(canonical_club_name(club));
    }
    for item in &podium_export.items {
        names.insert(canonical_club_name(&item.club));
        if !item.canonical_club.trim().is_empty() {
            names.insert(item.canonical_club.clone());
        }
    }
    for item in &participation_export.matches {
        names.insert(canonical_club_name(&item.club));
        if !item.canonical_club.trim().is_empty() {
            names.insert(item.canonical_club.clone());
        }
    }
    names
}

pub(super) fn resolve_truncated_club(club: &str, candidates: &BTreeSet<String>) -> String {
    let canonical = canonical_club_name(club);
    let Some(prefix) = truncated_prefix(&canonical) else {
        return canonical;
    };
    candidates
        .iter()
        .find(|candidate| candidate.starts_with(prefix))
        .cloned()
        .unwrap_or(canonical)
}

pub(super) fn source_display_name(source_name: &str) -> &str {
    if source_name.contains("landesmeisterschaften") || source_name == "LM" {
        "LM"
    } else if source_name.contains("deutsche-meisterschaften") || source_name == "DM" {
        "DM"
    } else {
        source_name
    }
}

pub(super) fn association_name(association_code: &str) -> &str {
    match association_code {
        "OD" => "Stormarn",
        "RZ" => "Herzogtum Lauenburg",
        "NF" => "Nordfriesland",
        "PI" => "Pinneberg",
        "SE" => "Segeberg",
        "RD" => "Rendsburg-Eckernförde",
        "KI" => "Kiel",
        "HL" => "Lübeck",
        "SL" => "Schleswig-Flensburg",
        "IZ" => "Steinburg",
        "HEI" => "Dithmarschen",
        "OH" => "Ostholstein",
        "PLÖ" => "Plön",
        "FL" => "Flensburg",
        "NMS" => "Neumünster",
        association => association,
    }
}

pub(super) fn association_label(code: &str, name: &str) -> String {
    if name.is_empty() || name == code {
        code.to_string()
    } else {
        format!("{code} - {name}")
    }
}

pub(super) const fn place_from_rank(rank: &Rank) -> Option<u32> {
    match rank {
        Rank::Place(place) => Some(*place),
        Rank::NotStarted | Rank::OutOfCompetition => None,
    }
}
