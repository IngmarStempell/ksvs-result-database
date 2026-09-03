use super::database::{DatabaseCombinedExporter, DatabasePodiumExporter};
use super::exporters::{CombinedExporter, PodiumExporter, apply_manual_overrides};
use super::html::{escape_html, render_html_export};
use super::matching::{
    association_label, association_matches, association_name, canonical_club_name, club_aliases,
    club_name_without_numeric_prefix, collapse_truncated_clubs, collapse_whitespace,
    combined_known_club_names, is_meyton_rank_header, meyton_association_code,
    meyton_continued_shooter_name, meyton_discipline_code, meyton_event_date, meyton_event_name,
    meyton_shooter_name, meyton_team_header, normalize_match_text, participation_shooter_from_line,
    participation_shooters, report_source_name, resolve_truncated_club, source_display_name,
    truncated_prefix,
};
use super::models::{
    CombinedExportConfig, DatabaseCombinedExportConfig, DatabasePodiumExportConfig,
    ManualNameOverrides, ManualReviewPdf, ParticipationExport, ParticipationMatch, PodiumExport,
    PodiumExportConfig, PodiumExportItem, PodiumResultKind,
};
use crate::import::{
    ParticipationImportConfig, ParticipationImporter, PodiumImportConfig, PodiumImporter,
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
        manual_overrides: ManualNameOverrides::default(),
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
        manual_overrides: ManualNameOverrides::default(),
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
        manual_overrides: ManualNameOverrides::default(),
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
        manual_overrides: ManualNameOverrides::default(),
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
            local_path: PathBuf::from("data/archive/2026/landesmeisterschaften/downloads/lm.pdf"),
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
fn applies_manual_name_overrides_to_podium_export_items() {
    let mut items = vec![PodiumExportItem {
        source_name: "landesmeisterschaften".to_string(),
        rank: 1,
        result_kind: PodiumResultKind::Individual,
        shooter: "R hl, Eberhard".to_string(),
        club: "080 Schützenverein Reinfeld 1".to_string(),
        canonical_club: "Schützenverein Reinfeld".to_string(),
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
    }];
    let overrides = ManualNameOverrides {
        club_names: BTreeMap::from([(
            "Schützenverein Reinfeld".to_string(),
            "Schützenverein Reinfeld e.V.".to_string(),
        )]),
        athlete_names: BTreeMap::from([(
            "R hl, Eberhard".to_string(),
            "Rühl, Eberhard".to_string(),
        )]),
    };

    apply_manual_overrides(&mut items, &overrides);

    assert_eq!(items[0].club, "080 Schützenverein Reinfeld 1");
    assert_eq!(items[0].canonical_club, "Schützenverein Reinfeld e.V.");
    assert_eq!(items[0].shooter, "Rühl, Eberhard");
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

#[tokio::test]
async fn exports_podium_and_combined_reports_from_database() {
    let dir = std::env::temp_dir().join(format!(
        "pdf-explorer-db-export-test-{}",
        Utc::now()
            .timestamp_nanos_opt()
            .expect("timestamp available")
    ));
    fs::create_dir_all(&dir).expect("test dir is created");
    let database_path = dir.join("db.sqlite");
    let podium_input_path = dir.join("podium-input.json");
    let participation_input_path = dir.join("participation-input.json");
    let db_podium_json = dir.join("db-podium.json");
    let db_podium_html = dir.join("db-podium.html");
    let db_combined_json = dir.join("db-combined.json");
    let db_combined_html = dir.join("db-combined.html");

    fs::write(
        &podium_input_path,
        serde_json::to_string_pretty(&database_podium_seed()).expect("podium serializes"),
    )
    .expect("podium input is written");
    fs::write(
        &participation_input_path,
        serde_json::to_string_pretty(&database_participation_seed())
            .expect("participation serializes"),
    )
    .expect("participation input is written");

    PodiumImporter::new(PodiumImportConfig {
        input_path: podium_input_path,
        database_path: database_path.clone(),
    })
    .run()
    .await
    .expect("podium import succeeds");
    ParticipationImporter::new(ParticipationImportConfig {
        input_path: participation_input_path,
        database_path: database_path.clone(),
    })
    .run()
    .await
    .expect("participation import succeeds");

    let podium_export = DatabasePodiumExporter::new(DatabasePodiumExportConfig {
        database_path: database_path.clone(),
        json_output_path: db_podium_json.clone(),
        html_output_path: db_podium_html.clone(),
        year: 2026,
        competition_scope: "LM".to_string(),
        focus_association_code: "OD".to_string(),
        max_place: 3,
    })
    .run()
    .await
    .expect("database podium export succeeds");
    let combined_export = DatabaseCombinedExporter::new(DatabaseCombinedExportConfig {
        database_path,
        json_output_path: db_combined_json.clone(),
        html_output_path: db_combined_html.clone(),
        year: 2026,
        focus_association_code: "OD".to_string(),
        max_place: 3,
    })
    .run()
    .await
    .expect("database combined export succeeds");

    assert_eq!(podium_export.item_count, 1);
    assert_eq!(combined_export.podium_item_count, 1);
    assert_eq!(combined_export.participation_match_count, 1);
    assert!(db_podium_json.exists());
    assert!(db_podium_html.exists());
    assert!(db_combined_json.exists());
    assert!(db_combined_html.exists());

    let _ = fs::remove_dir_all(dir);
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
    let shooter = meyton_continued_shooter_name(&prefix, "Christine Single Shot Series").unwrap();
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

fn database_podium_seed() -> PodiumExport {
    PodiumExport {
        generated_at: Utc::now(),
        source_report_path: PathBuf::from(
            "reports/archive/2026/landesmeisterschaften/crawl-report.json",
        ),
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
            class_name: Some("Damen I".to_string()),
            event_name: "Landesmeisterschaft 2026".to_string(),
            event_date: Some("01.06.2026".to_string()),
            score: Some(399.0),
            pdf_url: "https://example.org/lm.pdf".to_string(),
            local_path: PathBuf::from("data/archive/2026/landesmeisterschaften/downloads/lm.pdf"),
        }],
    }
}

fn database_participation_seed() -> ParticipationExport {
    ParticipationExport {
        generated_at: Utc::now(),
        club_source_report_path: PathBuf::from(
            "reports/archive/2026/landesmeisterschaften/crawl-report.json",
        ),
        results_report_path: PathBuf::from(
            "reports/archive/2026/deutsche-meisterschaften/crawl-report.json",
        ),
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
            local_path: PathBuf::from(
                "data/archive/2026/deutsche-meisterschaften/downloads/dm.pdf",
            ),
            text_char_count: 100,
        }],
    }
}
