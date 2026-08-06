use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SportResultList {
    pub event: EventInfo,
    pub team_results: Vec<TeamResult>,
    pub individual_results: Vec<IndividualResult>,
    pub out_of_competition_team_results: Vec<TeamResult>,
    pub out_of_competition_individual_results: Vec<IndividualResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventInfo {
    pub name: String,
    pub date: Option<String>,
    pub location: Option<String>,
    pub system: Option<String>,
    pub discipline_code: Option<String>,
    pub discipline: Option<String>,
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamResult {
    pub event: EventInfo,
    pub rank: Option<u32>,
    pub association: String,
    pub club: String,
    pub total: f32,
    pub members: Vec<TeamMemberResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamMemberResult {
    pub start_number: u32,
    pub name: String,
    pub total: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndividualResult {
    pub event: EventInfo,
    pub rank: Rank,
    pub start_number: u32,
    pub name: String,
    pub association: String,
    pub club: String,
    pub series: Vec<f32>,
    pub total: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Rank {
    Place(u32),
    NotStarted,
    OutOfCompetition,
}

#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub struct SportResultsParser {
    metadata_regex: Regex,
    team_regex: Regex,
    team_member_regex: Regex,
    individual_regex: Regex,
    individual_na_regex: Regex,
    individual_out_of_competition_regex: Regex,
}

impl SportResultsParser {
    /// Builds a DAVID21+ result parser.
    ///
    /// # Panics
    ///
    /// Panics only if one of the static regular expression literals is invalid.
    #[must_use]
    pub fn new() -> Self {
        Self {
            metadata_regex: Regex::new(
                r"^der am (?P<date>\d{2}\.\d{2}\.\d{4})\s+in (?P<location>.+?)\s+(?P<system>DV-System .+)$",
            )
            .expect("valid metadata regex"),
            team_regex: Regex::new(
                r"^(?:(?P<rank>\d+)\s+)?(?P<association>[A-Z0-9]{2})\s+(?P<club>.+?)\s+(?P<total>\d+(?:,\d)?)(?:\s+Ringen)?$",
            )
            .expect("valid team regex"),
            team_member_regex: Regex::new(
                r"^(?P<start_number>\d+)\s+(?P<name>.+?)\s+(?P<total>\d+(?:,\d)?)$",
            )
            .expect("valid team member regex"),
            individual_regex: Regex::new(
                r"^(?P<rank>\d+)\s+(?P<start_number>\d+)\s+(?P<name>.+?)\s+(?P<association>[A-Z0-9]{2})\s+(?P<club>.+?)\s+(?P<series>(?:\d+(?:,\d)?\s+)+)(?P<total>\d+(?:,\d)?)$",
            )
            .expect("valid individual regex"),
            individual_na_regex: Regex::new(
                r"^na\s+(?P<start_number>\d+)\s+(?P<name>.+?)\s+(?P<association>[A-Z0-9]{2})\s+(?P<club>.+?)\s+(?P<total>\d+,\d)$",
            )
            .expect("valid individual not-started regex"),
            individual_out_of_competition_regex: Regex::new(
                r"^(?P<start_number>\d+)\s+(?P<name>.+?)\s+(?P<association>[A-Z0-9]{2})\s+(?P<club>.+?)\s+(?P<series>(?:\d+(?:,\d)?\s+)+)(?P<total>\d+(?:,\d)?)$",
            )
            .expect("valid individual out-of-competition regex"),
        }
    }

    /// Parses extracted DAVID21+ PDF text into structured sport results.
    ///
    /// # Errors
    ///
    /// Returns an error if a recognized row contains invalid numeric values.
    pub fn parse(&self, text: &str) -> Result<SportResultList> {
        let lines = normalized_lines(text);
        let event = self.parse_event_info(&lines);
        let mut current_event = event.clone();
        let mut team_results = Vec::new();
        let mut individual_results = Vec::new();
        let mut out_of_competition_team_results = Vec::new();
        let mut out_of_competition_individual_results = Vec::new();
        let mut section = Section::Unknown;
        let mut out_of_competition = false;
        let mut current_team: Option<TeamResult> = None;
        let mut last_individual_place: Option<u32> = None;

        for line in lines {
            if line.starts_with("Ergebnisliste Mannschaft") {
                current_event.discipline_code =
                    line.split_whitespace().last().map(ToOwned::to_owned);
                flush_team(
                    &mut current_team,
                    out_of_competition,
                    &mut team_results,
                    &mut out_of_competition_team_results,
                );
                section = Section::Team;
                out_of_competition = false;
                last_individual_place = None;
                continue;
            }

            if line.starts_with("Ergebnisliste Einzel") {
                current_event.discipline_code =
                    line.split_whitespace().last().map(ToOwned::to_owned);
                flush_team(
                    &mut current_team,
                    out_of_competition,
                    &mut team_results,
                    &mut out_of_competition_team_results,
                );
                section = Section::Individual;
                out_of_competition = false;
                last_individual_place = None;
                continue;
            }

            if let Some((discipline, class_name)) = parse_discipline_class(&line) {
                current_event.discipline = Some(discipline);
                current_event.class_name = Some(class_name);
                continue;
            }

            if line.starts_with("Außer Konkurrenz") {
                flush_team(
                    &mut current_team,
                    out_of_competition,
                    &mut team_results,
                    &mut out_of_competition_team_results,
                );
                out_of_competition = true;
                last_individual_place = None;
                continue;
            }

            match section {
                Section::Team => {
                    if let Some(team) = self.parse_team(&line, &current_event)? {
                        flush_team(
                            &mut current_team,
                            out_of_competition,
                            &mut team_results,
                            &mut out_of_competition_team_results,
                        );
                        current_team = Some(team);
                    } else if let Some(member) = self.parse_team_member(&line)?
                        && let Some(team) = &mut current_team
                    {
                        team.members.push(member);
                    }
                }
                Section::Individual => {
                    if let Some(result) = self.parse_individual(&line, &current_event)? {
                        push_individual_result(
                            result,
                            out_of_competition,
                            &mut last_individual_place,
                            &mut individual_results,
                            &mut out_of_competition_individual_results,
                        );
                    }
                }
                Section::Unknown => {}
            }
        }

        flush_team(
            &mut current_team,
            out_of_competition,
            &mut team_results,
            &mut out_of_competition_team_results,
        );

        Ok(SportResultList {
            event,
            team_results,
            individual_results,
            out_of_competition_team_results,
            out_of_competition_individual_results,
        })
    }

    fn parse_event_info(&self, lines: &[String]) -> EventInfo {
        let mut event = EventInfo {
            name: lines.first().cloned().unwrap_or_default(),
            date: None,
            location: None,
            system: None,
            discipline_code: None,
            discipline: None,
            class_name: None,
        };

        for line in lines {
            if let Some(captures) = self.metadata_regex.captures(line) {
                event.date = Some(captures["date"].to_string());
                event.location = Some(captures["location"].trim().to_string());
                event.system = Some(captures["system"].trim().to_string());
            } else if line.starts_with("Ergebnisliste Mannschaft")
                || line.starts_with("Ergebnisliste Einzel")
            {
                if event.discipline_code.is_none() {
                    event.discipline_code = line.split_whitespace().last().map(ToOwned::to_owned);
                }
            } else if event.discipline.is_none()
                && let Some((discipline, class_name)) = parse_discipline_class(line)
            {
                event.discipline = Some(discipline);
                event.class_name = Some(class_name);
            }
        }

        event
    }

    fn parse_team(&self, line: &str, event: &EventInfo) -> Result<Option<TeamResult>> {
        let Some(captures) = self.team_regex.captures(line) else {
            return Ok(None);
        };

        Ok(Some(TeamResult {
            event: event.clone(),
            rank: captures
                .name("rank")
                .map(|rank| rank.as_str().parse())
                .transpose()
                .context("invalid team rank")?,
            association: captures["association"].to_string(),
            club: normalize_club_name(&captures["club"]),
            total: parse_decimal(&captures["total"])?,
            members: Vec::new(),
        }))
    }

    fn parse_team_member(&self, line: &str) -> Result<Option<TeamMemberResult>> {
        let Some(captures) = self.team_member_regex.captures(line) else {
            return Ok(None);
        };

        Ok(Some(TeamMemberResult {
            start_number: captures["start_number"]
                .parse()
                .context("invalid team member start number")?,
            name: captures["name"].trim().to_string(),
            total: parse_decimal(&captures["total"])?,
        }))
    }

    fn parse_individual(&self, line: &str, event: &EventInfo) -> Result<Option<IndividualResult>> {
        if let Some(captures) = self.individual_regex.captures(line) {
            return Ok(Some(IndividualResult {
                event: event.clone(),
                rank: Rank::Place(
                    captures["rank"]
                        .parse()
                        .context("invalid individual rank")?,
                ),
                start_number: captures["start_number"]
                    .parse()
                    .context("invalid individual start number")?,
                name: captures["name"].trim().to_string(),
                association: captures["association"].to_string(),
                club: normalize_club_name(&captures["club"]),
                series: parse_series(&captures["series"])?,
                total: parse_decimal(&captures["total"])?,
            }));
        }

        if let Some(captures) = self.individual_na_regex.captures(line) {
            return Ok(Some(IndividualResult {
                event: event.clone(),
                rank: Rank::NotStarted,
                start_number: captures["start_number"]
                    .parse()
                    .context("invalid individual start number")?,
                name: captures["name"].trim().to_string(),
                association: captures["association"].to_string(),
                club: normalize_club_name(&captures["club"]),
                series: Vec::new(),
                total: parse_decimal(&captures["total"])?,
            }));
        }

        if let Some(captures) = self.individual_out_of_competition_regex.captures(line) {
            return Ok(Some(IndividualResult {
                event: event.clone(),
                rank: Rank::OutOfCompetition,
                start_number: captures["start_number"]
                    .parse()
                    .context("invalid individual start number")?,
                name: captures["name"].trim().to_string(),
                association: captures["association"].to_string(),
                club: normalize_club_name(&captures["club"]),
                series: parse_series(&captures["series"])?,
                total: parse_decimal(&captures["total"])?,
            }));
        }

        Ok(None)
    }
}

impl Default for SportResultsParser {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
enum Section {
    Unknown,
    Team,
    Individual,
}

fn normalized_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_discipline_class(line: &str) -> Option<(String, String)> {
    let (left, _) = line.split_once(" Seite: ")?;
    let tokens = left.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }

    let class_start = tokens
        .iter()
        .position(|token| is_class_start_token(token))
        .unwrap_or(tokens.len() - 2);
    (class_start > 0).then(|| {
        (
            tokens[..class_start].join(" "),
            tokens[class_start..].join(" "),
        )
    })
}

fn is_class_start_token(token: &str) -> bool {
    matches!(
        token,
        "Herren" | "Damen" | "Schüler" | "Jugend" | "Junioren" | "Senioren" | "Parasportler"
    )
}

fn flush_team(
    current_team: &mut Option<TeamResult>,
    out_of_competition: bool,
    team_results: &mut Vec<TeamResult>,
    out_of_competition_team_results: &mut Vec<TeamResult>,
) {
    let Some(team) = current_team.take() else {
        return;
    };

    if out_of_competition {
        out_of_competition_team_results.push(team);
    } else {
        team_results.push(team);
    }
}

fn push_individual_result(
    mut result: IndividualResult,
    out_of_competition: bool,
    last_individual_place: &mut Option<u32>,
    individual_results: &mut Vec<IndividualResult>,
    out_of_competition_individual_results: &mut Vec<IndividualResult>,
) {
    inherit_tied_individual_rank(&mut result, out_of_competition, last_individual_place);
    if out_of_competition {
        out_of_competition_individual_results.push(result);
    } else {
        individual_results.push(result);
    }
}

const fn inherit_tied_individual_rank(
    result: &mut IndividualResult,
    out_of_competition: bool,
    last_individual_place: &mut Option<u32>,
) {
    if !out_of_competition
        && matches!(result.rank, Rank::OutOfCompetition)
        && let Some(rank) = *last_individual_place
    {
        result.rank = Rank::Place(rank);
    }

    if let Rank::Place(rank) = &result.rank {
        *last_individual_place = Some(*rank);
    }
}

fn parse_decimal(value: &str) -> Result<f32> {
    value
        .replace(',', ".")
        .parse()
        .with_context(|| format!("invalid decimal value {value}"))
}

fn parse_series(value: &str) -> Result<Vec<f32>> {
    value.split_whitespace().map(parse_decimal).collect()
}

fn normalize_club_name(value: &str) -> String {
    let trimmed = value.trim();
    let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    let without_extraction_prefix = collapsed
        .split_once("... ")
        .map_or(collapsed.as_str(), |(_, club)| club.trim());

    without_extraction_prefix
        .rsplit_once(' ')
        .filter(|(_, suffix)| is_team_suffix(suffix))
        .map_or(without_extraction_prefix, |(club, _)| club)
        .trim()
        .to_string()
}

fn is_team_suffix(value: &str) -> bool {
    matches!(
        value,
        "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X"
    )
}

#[cfg(test)]
mod tests {
    use super::{Rank, SportResultsParser, normalize_club_name};

    #[test]
    fn parses_team_and_individual_rows_from_david21_text() {
        let text = r"
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

        let result = SportResultsParser::new().parse(text).unwrap();

        assert_eq!(result.event.discipline_code.as_deref(), Some("1.10.10"));
        assert_eq!(result.team_results.len(), 1);
        assert_eq!(result.team_results[0].club, "SSV Kassau");
        assert_eq!(result.team_results[0].members.len(), 2);
        assert_eq!(result.individual_results.len(), 2);
        assert_eq!(result.out_of_competition_individual_results.len(), 1);
        assert_eq!(result.individual_results[0].rank, Rank::Place(1));
        assert_eq!(result.individual_results[0].club, "SSV Kassau");
        assert_eq!(result.individual_results[1].rank, Rank::NotStarted);
        assert_eq!(
            result.out_of_competition_individual_results[0].rank,
            Rank::OutOfCompetition
        );
    }

    #[test]
    fn parses_team_rows_without_ringen_suffix() {
        let text = r"
Landesmeisterschaft 2026
der am 30.05.2026          in Kellinghusen                               DV-System DAVID21+
Ergebnisliste Mannschaft                                                          1.15.20
Luftgewehr Liegendkampf                            Schüler I                      Seite:   1
Stand: 31.05.2026              11:37   Uhr                      Gesamt
1   RZ 011       Schwarzenbeker SchG I                       937,4
490 Arlt, Alexander                311,8
491 Janshen, Gotje                 314,1
492 Metzing, Emma                  311,5
3   OD 080       SchV Elmenhorst                             603,8
1039   Behrens, Joris Velten          0,0
1040   Knolinski, Sofia             303,1
1041   Roß, Liv                     300,7
Ergebnisliste Einzel                                                                            1.15.21
Luftgewehr Liegendkampf                            Schüler I weiblich                       Seite:    1
1    491 Janshen, Gotje                RZ 011   Schwarzenbeker SchG I   105,0 105,1 104,0       314,1
";

        let result = SportResultsParser::new().parse(text).unwrap();

        assert_eq!(result.team_results.len(), 2);
        assert_eq!(result.team_results[0].rank, Some(1));
        assert_eq!(result.team_results[0].association, "RZ");
        assert_eq!(result.team_results[0].club, "011 Schwarzenbeker SchG");
        assert_eq!(result.team_results[0].members.len(), 3);
        assert_eq!(result.team_results[1].rank, Some(3));
        assert_eq!(result.team_results[1].association, "OD");
        assert_eq!(result.team_results[1].club, "080 SchV Elmenhorst");
        assert_eq!(result.team_results[1].members.len(), 3);
    }

    #[test]
    fn parses_team_rows_with_integer_totals() {
        let text = r"
Landesmeisterschaft 2026
der am 13.06.2026        in Kellinghusen                              DV-System DAVID21+
Ergebnisliste Mannschaft                                                       1.20.20
Luftgewehr-3-Stellung                            Schüler I                     Seite:   1
Stand: 13.06.2026            17:48 Uhr                       Gesamt
1   OD 080       SchV Elmenhorst                          1595 Ringen
1039 Behrens, Joris Velten         503
1040 Knolinski, Sofia              543
1041 Roß, Liv                      549
2   OH 080       Lensahner SchG                           1534 Ringen
1193 Hamer, Theo                   519
1194 Meisburger, Oliver Marvin     511
1197 Schöning, Tjalf               504
Außer Konkurrenz haben geschossen:
OH 080       SSV Kassau                               1726 Ringen
907 Arlt, Alexander               555
908 Janshen, Gotje                589
906 Metzing, Emma                 582
Ergebnisliste Einzel                                                                                                    1.20.20
Luftgewehr-3-Stellung                        Schüler I                                                                  Seite:   1
1 1088 Gesswein, Malte         OD 012 SchV Reinfeld      89    86    175    95    95    190     88    90    178     543
";

        let result = SportResultsParser::new().parse(text).unwrap();

        assert_eq!(result.team_results.len(), 2);
        assert_eq!(result.team_results[0].rank, Some(1));
        assert_eq!(result.team_results[0].association, "OD");
        assert_eq!(result.team_results[0].club, "080 SchV Elmenhorst");
        assert!((result.team_results[0].total - 1595.0).abs() < f32::EPSILON);
        assert_eq!(result.team_results[0].members.len(), 3);
        assert!((result.team_results[0].members[0].total - 503.0).abs() < f32::EPSILON);
        assert_eq!(result.out_of_competition_team_results.len(), 1);
        assert_eq!(
            result.out_of_competition_team_results[0].club,
            "080 SSV Kassau"
        );
        assert_eq!(result.out_of_competition_team_results[0].rank, None);
    }

    #[test]
    fn parses_individual_rows_with_variable_series_count() {
        let text = r"
Landesmeisterschaft 2025
der am 06.07.2025     in Kellinghusen DV-System DAVID21+
Ergebnisliste Einzel 1.80.30
KK-Liegendkampf 50 m Jugend Seite: 1
2 770 Albers, Elia  OD  SchV Reinfeld 100,9 98,1 96,5 98,9 93,2 96,9 584,5
";

        let result = SportResultsParser::new().parse(text).unwrap();

        assert_eq!(result.individual_results.len(), 1);
        assert_eq!(result.individual_results[0].club, "SchV Reinfeld");
        assert_eq!(result.individual_results[0].series.len(), 6);
    }

    #[test]
    fn parses_tied_individual_rows_without_repeated_rank() {
        let text = r"
Landesmeisterschaft 2026
der am 27.06.2026     in Kellinghusen DV-System DAVID21+
Ergebnisliste Einzel 2.90.10
NDSB-Pistole/-Revolver Herren I Seite: 1
1 588 Peters, Helge       PI 013 SchV Quickborn-Renzel I 37 41 78
2 589 Dau, Reiner         PI 013 SchV Quickborn-Renzel I 37 39 76
1028 Scharnberg, Bettina  OD 012 SchV Bargteheide 37 39 76
4 703 Reckendorf, Dirk    RD 004 Erster Eckernförder SchV 37 36 73
";

        let result = SportResultsParser::new().parse(text).unwrap();
        let scharnberg = result
            .individual_results
            .iter()
            .find(|result| result.name == "Scharnberg, Bettina")
            .expect("tied individual result is parsed");

        assert_eq!(scharnberg.rank, Rank::Place(2));
        assert_eq!(scharnberg.association, "OD");
        assert_eq!(scharnberg.club, "012 SchV Bargteheide");
    }

    #[test]
    fn keeps_individual_rows_attached_to_their_page_header() {
        let text = r"
Landesmeisterschaft 2026
der am 13.06.2026     in Kellinghusen DV-System DAVID21+
Ergebnisliste Einzel 1.35.13
KK-Gewehr 100m Damen II Seite: 1
3 1036 Stempell, Christine OD 012 SchV Elmenhorst 87 90 94 271
Ergebnisliste Einzel 1.35.41
KK-Gewehr 100m Junioren I weiblich Seite: 1
1 438 Beneke, Lea Sophie HL 010 Lübecker SpSch 96 97 77 268
";

        let result = SportResultsParser::new().parse(text).unwrap();
        let stempell = result
            .individual_results
            .iter()
            .find(|result| result.name == "Stempell, Christine")
            .expect("Stempell result is parsed");

        assert_eq!(stempell.event.discipline_code.as_deref(), Some("1.35.13"));
        assert_eq!(stempell.event.discipline.as_deref(), Some("KK-Gewehr 100m"));
        assert_eq!(stempell.event.class_name.as_deref(), Some("Damen II"));
    }

    #[test]
    fn normalizes_club_names_for_team_suffixes_and_extraction_artifacts() {
        assert_eq!(
            normalize_club_name("Ahrensburger SchG I"),
            "Ahrensburger SchG"
        );
        assert_eq!(
            normalize_club_name("0... SchV Klein Wesenberg"),
            "SchV Klein Wesenberg"
        );
        assert_eq!(
            normalize_club_name("Sülfelder Schützengilde von 1888 e.V."),
            "Sülfelder Schützengilde von 1888 e.V."
        );
    }
}
