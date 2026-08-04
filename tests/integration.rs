use pdf_explorer::sport_results::{
    EventInfo, IndividualResult, Rank, SportResultList, SportResultsParser, TeamMemberResult,
    TeamResult,
};
use std::path::PathBuf;

#[test]
fn parse_full_david21_workflow() {
    let text = "\
Landesmeisterschaft 2025
der am 14.06.2025     in Kellinghusen DV-System DAVID21+
Ergebnisliste Mannschaft 1.10.10
Luftgewehr Herren I Seite: 1
1 OH  SSV Kassau I 1204,6 Ringen
430 Dietmayr, Markus  405,3
431 Jeger, Florian  406,3
Ergebnisliste Einzel 1.10.10
Luftgewehr Herren I Seite: 1
1 431 Jeger, Florian  OH  SSV Kassau I 98,2 103,2 99,1 105,8 406,3
na 926 Bechtel, Finn  14  SchV Wilster 0,0
Außer Konkurrenz haben geschossen:
436 Venohr, Paul  OH  SSV Kassau II 96,7 96,4 98,9 99,6 391,6
";

    let parser = SportResultsParser::new();
    let result = parser.parse(text).expect("parsing succeeds");

    assert_eq!(result.event.name, "Landesmeisterschaft 2025");
    assert_eq!(result.event.discipline_code.as_deref(), Some("1.10.10"));
    assert_eq!(result.team_results.len(), 1);
    assert_eq!(result.team_results[0].club, "SSV Kassau");
    assert_eq!(result.team_results[0].members.len(), 2);
    assert_eq!(result.individual_results.len(), 2);
    assert_eq!(result.out_of_competition_individual_results.len(), 1);
    assert_eq!(result.individual_results[0].rank, Rank::Place(1));
    assert_eq!(result.individual_results[1].rank, Rank::NotStarted);
    assert_eq!(
        result.out_of_competition_individual_results[0].rank,
        Rank::OutOfCompetition
    );
}

#[test]
fn parse_individual_result_with_multiple_series() {
    let text = "\
Landesmeisterschaft 2025
der am 06.07.2025     in Kellinghusen DV-System DAVID21+
Ergebnisliste Einzel 1.80.30
KK-Liegendkampf 50 m Jugend Seite: 1
2 770 Albers, Elia  OD  SchV Reinfeld 100,9 98,1 96,5 98,9 93,2 96,9 584,5
";

    let parser = SportResultsParser::new();
    let result = parser.parse(text).expect("parsing succeeds");

    assert_eq!(result.individual_results.len(), 1);
    assert_eq!(result.individual_results[0].club, "SchV Reinfeld");
    assert_eq!(result.individual_results[0].series.len(), 6);
    assert_eq!(result.individual_results[0].rank, Rank::Place(2));
}

#[test]
fn sport_result_list_serialize() {
    let event = EventInfo {
        name: "Test Event".to_string(),
        date: Some("01.01.2025".to_string()),
        location: Some("Kellinghusen".to_string()),
        system: Some("DV-System DAVID21+".to_string()),
        discipline_code: Some("1.10.10".to_string()),
        discipline: Some("Luftgewehr".to_string()),
        class_name: Some("Herren I".to_string()),
    };
    let result_list = SportResultList {
        event: event.clone(),
        team_results: vec![TeamResult {
            event: event.clone(),
            rank: Some(1),
            association: "OD".to_string(),
            club: "SchV Trittau".to_string(),
            total: 100.0,
            members: vec![TeamMemberResult {
                start_number: 1,
                name: "Shooter".to_string(),
                total: 100.0,
            }],
        }],
        individual_results: vec![IndividualResult {
            event,
            rank: Rank::Place(2),
            start_number: 2,
            name: "Individual Shooter".to_string(),
            association: "OD".to_string(),
            club: "SchV Trittau".to_string(),
            series: vec![98.0, 99.0, 100.0],
            total: 297.0,
        }],
        out_of_competition_team_results: Vec::new(),
        out_of_competition_individual_results: Vec::new(),
    };

    let json = serde_json::to_string(&result_list).expect("serialization succeeds");
    assert!(json.contains("Test Event"));
    assert!(json.contains("SchV Trittau"));
}

#[test]
fn extract_options_default_is_80_chars() {
    let options = pdf_explorer::pdf::ExtractOptions::default();
    assert_eq!(options.min_text_chars, 80);
}

#[test]
fn pdf_extractor_constructs_with_custom_options() {
    let options = pdf_explorer::pdf::ExtractOptions {
        min_text_chars: 120,
    };
    let extractor = pdf_explorer::pdf::PdfExtractor::new(options);
    let _ = extractor;
}

#[test]
fn crawl_config_constructs_with_required_fields() {
    let config = pdf_explorer::ingest::CrawlConfig {
        source_url: "https://example.org".to_string(),
        source_name: "test".to_string(),
        state_dir: PathBuf::from(".pdf-explorer"),
        download_dir: PathBuf::from("data/downloads"),
        manual_review_dir: PathBuf::from("data/manual-review"),
        report_path: PathBuf::from("reports/report.json"),
        html_report_path: PathBuf::from("reports/report.html"),
        focus: "Stormarn".to_string(),
        focus_association_code: "OD".to_string(),
        year: None,
        extra_pdf_urls: Vec::new(),
        max_depth: 1,
        max_pages: 25,
        min_text_chars: 80,
    };

    assert_eq!(config.source_url, "https://example.org");
    assert_eq!(config.focus_association_code, "OD");
}
