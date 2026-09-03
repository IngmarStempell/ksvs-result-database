use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::template::render_template;

use super::matching::{association_label, source_display_name};
use super::models::{
    CombinedExport, ManualReviewPdf, ParticipationExport, PodiumExport, PodiumExportItem,
    PodiumResultKind,
};

const PODIUM_EXPORT_TEMPLATE: &str = include_str!("../../templates/podium-export.html");
const PODIUM_MANUAL_REVIEW_TABLE_TEMPLATE: &str =
    include_str!("../../templates/podium-manual-review-table.html");
const PARTICIPATION_EXPORT_TEMPLATE: &str =
    include_str!("../../templates/participation-export.html");
const COMBINED_EXPORT_TEMPLATE: &str = include_str!("../../templates/combined-export.html");

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

pub(super) fn render_html_export(export: &PodiumExport, _report_dir: Option<&Path>) -> String {
    let mut rows = String::new();
    let highest_rank = export
        .items
        .iter()
        .map(|item| item.rank)
        .max()
        .unwrap_or(export.max_place);
    let items_json = serde_json::to_string(
        &export
            .items
            .iter()
            .map(PodiumHtmlItem::from)
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());
    let manual_review_section = if export.manual_review_pdfs.is_empty() {
        String::new()
    } else {
        render_template(
            PODIUM_MANUAL_REVIEW_TABLE_TEMPLATE,
            &[(
                "manual_review_rows",
                render_manual_review_rows(&export.manual_review_pdfs),
            )],
        )
    };

    for item in &export.items {
        let _ = writeln!(
            rows,
            "<tr data-rank=\"{}\" data-kind=\"{}\" data-shooter=\"{}\" data-club=\"{}\" data-canonical-club=\"{}\" data-association=\"{}\" data-text=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            item.rank,
            result_kind_label(&item.result_kind),
            escape_html(&item.shooter),
            escape_html(&item.club),
            escape_html(&item.canonical_club),
            escape_html(&item.association_code),
            escape_html(&format!(
                "{} {} {} {} {} {}",
                item.shooter,
                item.club,
                item.canonical_club,
                item.association_code,
                item.discipline.as_deref().unwrap_or_default(),
                item.pdf_url
            )),
            item.rank,
            escape_html(result_kind_label(&item.result_kind)),
            escape_html(&item.shooter),
            escape_html(&item.canonical_club),
            escape_html(&association_label(
                &item.association_code,
                &item.association_name
            )),
            discipline_cell(item),
            score_cell(item.score),
            link_html(&item.pdf_url, &item.pdf_url)
        );
    }

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

pub(super) fn render_participation_html(
    export: &ParticipationExport,
    report_dir: Option<&Path>,
) -> String {
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

pub(super) fn render_combined_html(export: &CombinedExport, report_dir: Option<&Path>) -> String {
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

pub(super) fn manual_review_pdfs(
    crawl_report: &crate::ingest::CrawlReport,
) -> Vec<ManualReviewPdf> {
    crawl_report
        .pdfs
        .iter()
        .filter(|pdf| pdf.classification != crate::ingest::PdfClassification::David21)
        .map(|pdf| ManualReviewPdf {
            url: pdf.url.clone(),
            reason: pdf
                .error
                .clone()
                .or_else(|| Some(format!("{:?}", pdf.classification))),
            text_char_count: pdf.text_char_count,
            needs_ocr: pdf.needs_ocr,
        })
        .collect()
}

fn render_manual_review_rows(pdfs: &[ManualReviewPdf]) -> String {
    let mut rows = String::new();
    for pdf in pdfs {
        let reason = pdf.reason.as_deref().unwrap_or("unbekannt");
        let text_char_count = pdf
            .text_char_count
            .map_or_else(String::new, |count| count.to_string());
        let needs_ocr = pdf
            .needs_ocr
            .map_or("", |needs_ocr| if needs_ocr { "ja" } else { "nein" });
        let _ = writeln!(
            rows,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            link_html(&pdf.url, &pdf.url),
            escape_html(reason),
            escape_html(&text_char_count),
            escape_html(needs_ocr)
        );
    }
    rows
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

fn shooters_display(shooters: &[String]) -> String {
    shooters.join(", ")
}

fn link_html(href: &str, label: &str) -> String {
    let href = escape_html(href);
    let label = escape_html(label);
    format!("<a href=\"{href}\"><code>{label}</code></a>")
}

const fn result_kind_label(kind: &PodiumResultKind) -> &'static str {
    match kind {
        PodiumResultKind::Individual => "Einzel",
        PodiumResultKind::Team => "Mannschaft",
    }
}

fn local_path_link(path: &Path, report_dir: Option<&Path>) -> String {
    relative_local_href(path, report_dir).map_or_else(
        || escape_html(&path.to_string_lossy()),
        |href| {
            let label = escape_html(&path.to_string_lossy());
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

pub(super) fn escape_html(value: &str) -> String {
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
    use std::path::PathBuf;

    use chrono::Utc;

    use super::{escape_html, render_html_export};
    use crate::export::{ManualReviewPdf, PodiumExport, PodiumExportItem, PodiumResultKind};

    #[test]
    fn escapes_html_and_control_characters() {
        assert_eq!(escape_html("A&B<\0>"), "A&amp;B&lt; &gt;");
    }

    #[test]
    fn renders_association_grouping_option() {
        let export = empty_export();
        let html = render_html_export(&export, None);
        assert!(html.contains("<option value=\"association\">Kreis</option>"));
    }

    #[test]
    fn renders_manual_review_summary_in_podium_html() {
        let mut export = empty_export();
        export.manual_review_count = 1;
        export.manual_review_pdfs = vec![ManualReviewPdf {
            url: "https://example.org/manual.pdf".to_string(),
            reason: Some("kein DAVID21+ Format".to_string()),
            text_char_count: Some(42),
            needs_ocr: Some(true),
        }];

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
        let mut export = empty_export();
        export.item_count = 1;
        export.items = vec![item()];

        let html = render_html_export(&export, None);

        assert!(!html.contains("<th>Lokale Datei</th>"));
        assert!(!html.contains("local_path"));
        assert!(!html.contains("data/archive/2026/landesmeisterschaften/downloads"));
        assert!(html.contains("https://example.org/lm.pdf"));
    }

    fn empty_export() -> PodiumExport {
        PodiumExport {
            generated_at: Utc::now(),
            source_report_path: PathBuf::from("reports/lm.json"),
            source_name: "landesmeisterschaften".to_string(),
            focus_association_code: "all".to_string(),
            max_place: 3,
            item_count: 0,
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items: Vec::new(),
        }
    }

    fn item() -> PodiumExportItem {
        PodiumExportItem {
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
            local_path: PathBuf::from("data/archive/2026/landesmeisterschaften/downloads/lm.pdf"),
        }
    }
}
