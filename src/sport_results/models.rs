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
