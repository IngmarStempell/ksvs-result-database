use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use anyhow::{Context, Result};
use percent_encoding::percent_decode_str;
use url::form_urlencoded;

use crate::application::{
    ApplicationService, AthleteFilters, ClubFilters, CombinedEvaluationRow, PageParams,
    ParsedIssueRow, ResultFilters, ResultRow, TeamFilters, TeamMemberRow, TeamRow,
};
use crate::storage::{
    Database, DatabaseConfig, NewClub, NewClubAlias, NewManualOverride, StorageRepository,
};
use crate::template::render_template;

const LAYOUT_TEMPLATE: &str = include_str!("../templates/web-layout.html");
const IMPORT_RUNS_TEMPLATE: &str = include_str!("../templates/web-import-runs.html");
const IMPORT_RUN_RESULTS_TEMPLATE: &str = include_str!("../templates/web-import-run-results.html");
const RESULTS_TEMPLATE: &str = include_str!("../templates/web-results.html");
const ATHLETES_TEMPLATE: &str = include_str!("../templates/web-athletes.html");
const CLUBS_TEMPLATE: &str = include_str!("../templates/web-clubs.html");
const TEAMS_TEMPLATE: &str = include_str!("../templates/web-teams.html");
const TEAM_DETAIL_TEMPLATE: &str = include_str!("../templates/web-team-detail.html");
const HONORS_TEMPLATE: &str = include_str!("../templates/web-honors.html");
const CORRECTIONS_TEMPLATE: &str = include_str!("../templates/web-corrections.html");
const PARSER_ISSUES_TEMPLATE: &str = include_str!("../templates/web-parser-issues.html");
const SOURCES_TEMPLATE: &str = include_str!("../templates/web-sources.html");
const SOURCE_DETAIL_TEMPLATE: &str = include_str!("../templates/web-source-detail.html");
const ATHLETE_DETAIL_TEMPLATE: &str = include_str!("../templates/web-athlete-detail.html");
const CLUB_DETAIL_TEMPLATE: &str = include_str!("../templates/web-club-detail.html");
const PARSER_RUNS_TEMPLATE: &str = include_str!("../templates/web-parser-runs.html");
const PARSER_RUN_DETAIL_TEMPLATE: &str = include_str!("../templates/web-parser-run-detail.html");
const COMBINED_TEMPLATE: &str = include_str!("../templates/web-combined.html");
const CLUB_ALIASES_TEMPLATE: &str = include_str!("../templates/web-club-aliases.html");
const DEFAULT_PAGE_SIZE: usize = 100;
const PAGE_SIZE_OPTIONS: &[usize] = &[50, 100, 250, 500, 1_000];

#[derive(Debug, Clone, Copy)]
struct PageState {
    page: usize,
    page_size: usize,
}

#[derive(Debug, Clone)]
pub struct WebConfig {
    pub database_path: PathBuf,
    pub bind: String,
}

#[derive(Debug, Clone)]
pub struct WebServer {
    config: WebConfig,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    body: String,
}

enum WebResponse {
    Html(String),
    Redirect(&'static str),
    NotFound,
    MethodNotAllowed,
    InternalError(String),
}

impl WebServer {
    #[must_use]
    pub const fn new(config: WebConfig) -> Self {
        Self { config }
    }

    /// Starts a local read-only web UI backed by the configured `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened, the socket cannot be
    /// bound, or a response cannot be written.
    pub async fn run(&self) -> Result<()> {
        let database = Database::new(DatabaseConfig {
            path: self.config.database_path.clone(),
        });
        let pool = database.migrated_pool().await?;
        let listener = TcpListener::bind(&self.config.bind)
            .with_context(|| format!("could not bind web UI to {}", self.config.bind))?;

        println!("web UI running at http://{}", self.config.bind);
        println!("using database {}", self.config.database_path.display());

        for stream in listener.incoming() {
            let mut stream = stream.context("could not accept web UI connection")?;
            let response = handle_stream(&mut stream, &pool).await;
            write_response(&mut stream, response)?;
        }

        pool.close().await;
        Ok(())
    }
}

async fn handle_stream(stream: &mut TcpStream, pool: &sqlx::SqlitePool) -> WebResponse {
    let Ok(request) = read_http_request(stream) else {
        return WebResponse::MethodNotAllowed;
    };
    if !matches!(request.method.as_str(), "GET" | "POST") {
        return WebResponse::MethodNotAllowed;
    }

    match route(&request, pool).await {
        Ok(response) => response,
        Err(error) => WebResponse::InternalError(error.to_string()),
    }
}

async fn route(request: &HttpRequest, pool: &sqlx::SqlitePool) -> Result<WebResponse> {
    if request.method == "POST" {
        return route_post(request, pool).await;
    }

    route_get(&request.path, &request.query, pool).await
}

async fn route_get(
    path: &str,
    query: &BTreeMap<String, String>,
    pool: &sqlx::SqlitePool,
) -> Result<WebResponse> {
    match path {
        "/" => Ok(WebResponse::Redirect("/import-runs")),
        "/import-runs" => import_runs_page(pool).await.map(WebResponse::Html),
        "/results" => results_page(pool, query).await.map(WebResponse::Html),
        "/athletes" => athletes_page(pool, query).await.map(WebResponse::Html),
        "/clubs" => clubs_page(pool, query).await.map(WebResponse::Html),
        "/teams" => teams_page(pool, query).await.map(WebResponse::Html),
        "/sources" => sources_page(pool).await.map(WebResponse::Html),
        "/parser-runs" => parser_runs_page(pool).await.map(WebResponse::Html),
        "/combined" => combined_page(pool, query).await.map(WebResponse::Html),
        "/club-aliases" => club_aliases_page(pool).await.map(WebResponse::Html),
        "/corrections" => corrections_page(pool, query).await.map(WebResponse::Html),
        "/corrections/issues" => parser_issues_page(pool).await.map(WebResponse::Html),
        "/honors" => Ok(WebResponse::Html(honors_page())),
        path if path.starts_with("/athletes/") => {
            let id = path
                .trim_start_matches("/athletes/")
                .parse::<i64>()
                .context("invalid athlete id")?;
            athlete_detail_page(pool, id).await.map(WebResponse::Html)
        }
        path if path.starts_with("/clubs/") => {
            let id = path
                .trim_start_matches("/clubs/")
                .parse::<i64>()
                .context("invalid club id")?;
            club_detail_page(pool, id).await.map(WebResponse::Html)
        }
        path if path.starts_with("/teams/") => {
            let id = path
                .trim_start_matches("/teams/")
                .parse::<i64>()
                .context("invalid team id")?;
            team_detail_page(pool, id).await.map(WebResponse::Html)
        }
        path if path.starts_with("/sources/") => {
            let id = path
                .trim_start_matches("/sources/")
                .parse::<i64>()
                .context("invalid source document id")?;
            source_detail_page(pool, id).await.map(WebResponse::Html)
        }
        path if path.starts_with("/parser-runs/") => {
            let id = path
                .trim_start_matches("/parser-runs/")
                .parse::<i64>()
                .context("invalid parser run id")?;
            parser_run_detail_page(pool, id)
                .await
                .map(WebResponse::Html)
        }
        path if path.starts_with("/import-runs/") && path.ends_with("/results") => {
            let id = path
                .trim_start_matches("/import-runs/")
                .trim_end_matches("/results")
                .parse::<i64>()
                .context("invalid import run id")?;
            import_run_results_page(pool, id)
                .await
                .map(WebResponse::Html)
        }
        _ => Ok(WebResponse::NotFound),
    }
}

async fn route_post(request: &HttpRequest, pool: &sqlx::SqlitePool) -> Result<WebResponse> {
    match request.path.as_str() {
        "/corrections" => {
            create_manual_override(pool, &request.body).await?;
            Ok(WebResponse::Redirect("/corrections"))
        }
        "/club-aliases" => {
            create_club_alias(pool, &request.body).await?;
            Ok(WebResponse::Redirect("/club-aliases"))
        }
        path if path.starts_with("/club-aliases/") && path.ends_with("/deactivate") => {
            let id = path
                .trim_start_matches("/club-aliases/")
                .trim_end_matches("/deactivate")
                .parse::<i64>()
                .context("invalid club alias id")?;
            deactivate_club_alias(pool, id).await?;
            Ok(WebResponse::Redirect("/club-aliases"))
        }
        path if path.starts_with("/corrections/") && path.ends_with("/revoke") => {
            let id = path
                .trim_start_matches("/corrections/")
                .trim_end_matches("/revoke")
                .parse::<i64>()
                .context("invalid manual override id")?;
            revoke_manual_override(pool, id).await?;
            Ok(WebResponse::Redirect("/corrections"))
        }
        _ => Ok(WebResponse::NotFound),
    }
}

async fn import_runs_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = ApplicationService::new(pool).import_runs().await?;

    let mut rows_html = String::new();
    for row in &rows {
        let _ = writeln!(
            rows_html,
            "<tr><td><a href=\"/import-runs/{}/results\">#{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td></tr>",
            row.id,
            row.id,
            escape_html(&row.source_name),
            escape_html(&row.run_kind),
            parser_label(row.parser_name.as_deref(), row.parser_version.as_deref()),
            escape_html(&row.status),
            escape_optional(row.input_path.as_deref()),
            escape_html(&format_time_range(
                &row.started_at,
                row.finished_at.as_deref()
            )),
            row.result_count
        );
    }

    Ok(render_page(
        "Importlaeufe",
        "import-runs",
        render_template(IMPORT_RUNS_TEMPLATE, &[("rows", empty_rows(rows_html, 8))]),
    ))
}

async fn import_run_results_page(pool: &sqlx::SqlitePool, import_run_id: i64) -> Result<String> {
    let rows = ApplicationService::new(pool)
        .results(&ResultFilters {
            import_run_id: Some(import_run_id),
            ..ResultFilters::default()
        })
        .await?;
    let rows_html = result_rows_html(&rows);
    Ok(render_page(
        &format!("Importlauf #{import_run_id}"),
        "import-runs",
        render_template(
            IMPORT_RUN_RESULTS_TEMPLATE,
            &[
                ("import_run_id", import_run_id.to_string()),
                ("rows", empty_rows(rows_html, 10)),
            ],
        ),
    ))
}

async fn results_page(pool: &sqlx::SqlitePool, query: &BTreeMap<String, String>) -> Result<String> {
    let page = page_state(query);
    let filters = result_filters(query, page);
    let mut rows = ApplicationService::new(pool).results(&filters).await?;
    let has_next = trim_page_rows(&mut rows, page);
    Ok(render_page(
        "Ergebnisse",
        "results",
        render_template(
            RESULTS_TEMPLATE,
            &[
                ("filters", result_filter_form(&filters, "/results", page)),
                ("rows", empty_rows(result_rows_html(&rows), 10)),
                (
                    "pagination",
                    pagination_controls("/results", query, page, rows.len(), has_next),
                ),
            ],
        ),
    ))
}

async fn athletes_page(
    pool: &sqlx::SqlitePool,
    query: &BTreeMap<String, String>,
) -> Result<String> {
    let page = page_state(query);
    let filters = athlete_filters(query, page);
    let mut rows = ApplicationService::new(pool).athletes(&filters).await?;
    let has_next = trim_page_rows(&mut rows, page);

    let mut rows_html = String::new();
    for row in &rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td><a href=\"/athletes/{}\">{}</a></td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            row.id,
            row.id,
            escape_html(&row.canonical_name),
            row.result_count,
            row.club_count,
            row.latest_year
                .map_or_else(String::new, |year| year.to_string())
        );
    }

    Ok(render_page(
        "Sportler",
        "athletes",
        render_template(
            ATHLETES_TEMPLATE,
            &[
                ("filters", athlete_filter_form(&filters, "/athletes", page)),
                ("rows", empty_rows(rows_html, 5)),
                (
                    "pagination",
                    pagination_controls("/athletes", query, page, rows.len(), has_next),
                ),
            ],
        ),
    ))
}

async fn clubs_page(pool: &sqlx::SqlitePool, query: &BTreeMap<String, String>) -> Result<String> {
    let page = page_state(query);
    let filters = club_filters(query, page);
    let mut rows = ApplicationService::new(pool).clubs(&filters).await?;
    let has_next = trim_page_rows(&mut rows, page);

    let mut rows_html = String::new();
    for row in &rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td><a href=\"/clubs/{}\">{}</a></td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            row.id,
            row.id,
            escape_html(&row.canonical_name),
            escape_optional(row.association_code.as_deref()),
            row.result_count,
            row.athlete_count,
            row.latest_year
                .map_or_else(String::new, |year| year.to_string())
        );
    }

    Ok(render_page(
        "Vereine",
        "clubs",
        render_template(
            CLUBS_TEMPLATE,
            &[
                ("filters", club_filter_form(&filters, "/clubs", page)),
                ("rows", empty_rows(rows_html, 6)),
                (
                    "pagination",
                    pagination_controls("/clubs", query, page, rows.len(), has_next),
                ),
            ],
        ),
    ))
}

async fn teams_page(pool: &sqlx::SqlitePool, query: &BTreeMap<String, String>) -> Result<String> {
    let page = page_state(query);
    let filters = team_filters(query, page);
    let mut rows = ApplicationService::new(pool).teams(&filters).await?;
    let has_next = trim_page_rows(&mut rows, page);

    Ok(render_page(
        "Mannschaften",
        "teams",
        render_template(
            TEAMS_TEMPLATE,
            &[
                ("filters", team_filter_form(&filters, "/teams", page)),
                ("rows", empty_rows(team_rows_html(&rows), 11)),
                (
                    "pagination",
                    pagination_controls("/teams", query, page, rows.len(), has_next),
                ),
            ],
        ),
    ))
}

async fn corrections_page(
    pool: &sqlx::SqlitePool,
    query: &BTreeMap<String, String>,
) -> Result<String> {
    let service = ApplicationService::new(pool);
    let overrides = service.manual_overrides().await?;

    let active_count = overrides
        .iter()
        .filter(|manual_override| manual_override.status == "active")
        .count();
    let mut rows_html = String::new();
    for row in &overrides {
        let action = if row.status == "active" {
            format!(
                "<form class=\"inline\" method=\"post\" action=\"/corrections/{}/revoke\"><button type=\"submit\">Zuruecknehmen</button></form>",
                row.id
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            row.id,
            entity_type_label(&row.entity_type),
            escape_html(&row.field_name),
            escape_html(&row.scope),
            escape_html(&row.old_value),
            escape_html(&row.new_value),
            escape_optional(row.reason.as_deref()),
            escape_html(&row.status),
            escape_html(&row.created_at),
            escape_html(&row.updated_at),
            action
        );
    }
    let club_options = datalist_options(&service.club_names().await?);
    let athlete_options = datalist_options(&service.athlete_names().await?);

    Ok(render_page(
        "Korrekturen",
        "corrections",
        render_template(
            CORRECTIONS_TEMPLATE,
            &[
                ("active_count", active_count.to_string()),
                ("total_count", overrides.len().to_string()),
                (
                    "club_selected",
                    selected_attr(query.get("entity_type"), "club"),
                ),
                (
                    "athlete_selected",
                    selected_attr(query.get("entity_type"), "athlete"),
                ),
                (
                    "prefill_old_value",
                    escape_html(query.get("old_value").map_or("", String::as_str)),
                ),
                ("club_options", club_options),
                ("athlete_options", athlete_options),
                ("rows", empty_rows(rows_html, 11)),
            ],
        ),
    ))
}

async fn parser_issues_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = ApplicationService::new(pool).parser_issues().await?;

    let mut rows_html = String::new();
    for row in &rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td>{} {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            row.id,
            escape_html(&row.source_name),
            escape_html(&row.competition_scope),
            row.competition_year,
            escape_html(&row.conflict_status),
            issue_name_cell(
                row.raw_shooter_name.as_deref(),
                row.normalized_shooter_name.as_deref()
            ),
            issue_name_cell(
                row.raw_club_name.as_deref(),
                row.normalized_club_name.as_deref()
            ),
            discipline_issue_cell(row),
            escape_optional(row.event_name.as_deref()),
            source_link(row.pdf_url.as_deref()),
            correction_prefill_links(row)
        );
    }

    Ok(render_page(
        "Parserfaelle",
        "corrections",
        render_template(
            PARSER_ISSUES_TEMPLATE,
            &[
                ("issue_count", rows.len().to_string()),
                ("rows", empty_rows(rows_html, 10)),
            ],
        ),
    ))
}

async fn sources_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = ApplicationService::new(pool).source_documents().await?;
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td><a href=\"/sources/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            row.id,
            row.id,
            escape_html(&row.source_name),
            source_link(Some(&row.url)),
            escape_html(&row.classification),
            escape_optional(row.sha256.as_deref()),
            row.result_count,
            row.parser_row_count
        );
    }

    Ok(render_page(
        "Quellen",
        "sources",
        render_template(SOURCES_TEMPLATE, &[("rows", empty_rows(rows_html, 7))]),
    ))
}

async fn source_detail_page(pool: &sqlx::SqlitePool, source_id: i64) -> Result<String> {
    let service = ApplicationService::new(pool);
    let Some(source) = service.source_document(source_id).await? else {
        return Ok(render_page(
            "Quelle",
            "sources",
            "<h1>Quelle nicht gefunden</h1>".to_string(),
        ));
    };
    let rows = service
        .results(&ResultFilters {
            source_document_id: Some(source_id),
            ..ResultFilters::default()
        })
        .await?;
    Ok(render_page(
        &format!("Quelle #{}", source.id),
        "sources",
        render_template(
            SOURCE_DETAIL_TEMPLATE,
            &[
                ("source_id", source.id.to_string()),
                ("source_name", escape_html(&source.source_name)),
                ("source_url", source_link(Some(&source.url))),
                ("local_path", escape_optional(source.local_path.as_deref())),
                ("classification", escape_html(&source.classification)),
                ("sha256", escape_optional(source.sha256.as_deref())),
                ("result_count", source.result_count.to_string()),
                ("parser_row_count", source.parser_row_count.to_string()),
                ("rows", empty_rows(result_rows_html(&rows), 10)),
            ],
        ),
    ))
}

async fn athlete_detail_page(pool: &sqlx::SqlitePool, athlete_id: i64) -> Result<String> {
    let filters = ResultFilters {
        athlete_id: Some(athlete_id),
        ..ResultFilters::default()
    };
    let rows = ApplicationService::new(pool).results(&filters).await?;
    let athlete_name = rows
        .iter()
        .find_map(|row| row.athlete_name.as_deref())
        .map_or_else(|| format!("#{athlete_id}"), ToOwned::to_owned);
    Ok(render_page(
        &athlete_name,
        "athletes",
        render_template(
            ATHLETE_DETAIL_TEMPLATE,
            &[
                ("athlete_id", athlete_id.to_string()),
                ("athlete_name", escape_html(&athlete_name)),
                ("result_count", rows.len().to_string()),
                ("rows", empty_rows(result_rows_html(&rows), 10)),
            ],
        ),
    ))
}

async fn club_detail_page(pool: &sqlx::SqlitePool, club_id: i64) -> Result<String> {
    let filters = ResultFilters {
        club_id: Some(club_id),
        ..ResultFilters::default()
    };
    let rows = ApplicationService::new(pool).results(&filters).await?;
    let club_name = rows
        .iter()
        .find_map(|row| row.club_name.as_deref())
        .map_or_else(|| format!("#{club_id}"), ToOwned::to_owned);
    Ok(render_page(
        &club_name,
        "clubs",
        render_template(
            CLUB_DETAIL_TEMPLATE,
            &[
                ("club_id", club_id.to_string()),
                ("club_name", escape_html(&club_name)),
                ("result_count", rows.len().to_string()),
                ("rows", empty_rows(result_rows_html(&rows), 10)),
            ],
        ),
    ))
}

async fn team_detail_page(pool: &sqlx::SqlitePool, team_id: i64) -> Result<String> {
    let service = ApplicationService::new(pool);
    let Some(team) = service.team(team_id).await? else {
        return Ok(render_page(
            "Mannschaft",
            "teams",
            "<h1>Mannschaft nicht gefunden</h1>".to_string(),
        ));
    };
    let members = service.team_members(team_id).await?;
    Ok(render_page(
        &team.canonical_name,
        "teams",
        render_template(
            TEAM_DETAIL_TEMPLATE,
            &[
                ("team_id", team.id.to_string()),
                ("team_name", escape_html(&team.canonical_name)),
                ("team_number", escape_optional(team.team_number.as_deref())),
                (
                    "raw_team_name",
                    escape_optional(team.raw_team_name.as_deref()),
                ),
                (
                    "club",
                    detail_link("/clubs", team.club_id, team.club_name.as_deref()),
                ),
                (
                    "competition",
                    format!(
                        "{} {} - {}",
                        escape_html(&team.competition_scope),
                        team.competition_year,
                        escape_html(&team.competition_name)
                    ),
                ),
                ("discipline", escape_optional(team.discipline.as_deref())),
                ("event_class", escape_optional(team.event_class.as_deref())),
                (
                    "rank",
                    team.rank.map_or_else(String::new, |rank| rank.to_string()),
                ),
                ("score", team.score.map_or_else(String::new, format_score)),
                ("medal", escape_optional(team.medal.as_deref())),
                (
                    "source",
                    source_detail_link(team.source_document_id, team.source_url.as_deref()),
                ),
                ("member_count", team.member_count.to_string()),
                ("rows", empty_rows(team_member_rows_html(&members), 7)),
            ],
        ),
    ))
}

async fn parser_runs_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = ApplicationService::new(pool).parser_runs().await?;
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\"><a href=\"/parser-runs/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            row.id,
            row.id,
            escape_html(&row.source_name),
            escape_html(&row.source_kind),
            parser_label(Some(&row.parser_name), Some(&row.parser_version)),
            escape_html(&row.status),
            escape_html(&format_time_range(
                &row.started_at,
                row.finished_at.as_deref()
            )),
            escape_html(&row.input_path),
            row.parsed_row_count,
            row.issue_count
        );
    }

    Ok(render_page(
        "Parserlaeufe",
        "parser-runs",
        render_template(PARSER_RUNS_TEMPLATE, &[("rows", empty_rows(rows_html, 9))]),
    ))
}

async fn parser_run_detail_page(pool: &sqlx::SqlitePool, parser_run_id: i64) -> Result<String> {
    let service = ApplicationService::new(pool);
    let Some(row) = service.parser_run(parser_run_id).await? else {
        return Ok(render_page(
            "Parserlauf",
            "parser-runs",
            "<h1>Parserlauf nicht gefunden</h1>".to_string(),
        ));
    };
    Ok(render_page(
        &format!("Parserlauf #{}", row.id),
        "parser-runs",
        render_template(
            PARSER_RUN_DETAIL_TEMPLATE,
            &[
                ("parser_run_id", row.id.to_string()),
                ("source_name", escape_html(&row.source_name)),
                ("source_kind", escape_html(&row.source_kind)),
                (
                    "parser",
                    parser_label(Some(&row.parser_name), Some(&row.parser_version)),
                ),
                ("status", escape_html(&row.status)),
                (
                    "time_range",
                    escape_html(&format_time_range(
                        &row.started_at,
                        row.finished_at.as_deref(),
                    )),
                ),
                ("input_path", escape_html(&row.input_path)),
                ("parsed_row_count", row.parsed_row_count.to_string()),
                ("issue_count", row.issue_count.to_string()),
            ],
        ),
    ))
}

async fn combined_page(
    pool: &sqlx::SqlitePool,
    query: &BTreeMap<String, String>,
) -> Result<String> {
    let page = page_state(query);
    let filters = ResultFilters {
        search: non_empty_query(query, "q"),
        year: query_i64(query, "year"),
        association_code: non_empty_query(query, "verein"),
        page: PageParams {
            limit: page_fetch_limit(page),
            offset: page_offset(page),
        },
        ..ResultFilters::default()
    };
    let mut rows = ApplicationService::new(pool)
        .combined_evaluation(&filters)
        .await?;
    let has_next = trim_page_rows(&mut rows, page);
    Ok(render_page(
        "LM und DM",
        "combined",
        render_template(
            COMBINED_TEMPLATE,
            &[
                ("filters", combined_filter_form(&filters, "/combined", page)),
                ("rows", empty_rows(combined_rows_html(&rows), 9)),
                (
                    "pagination",
                    pagination_controls("/combined", query, page, rows.len(), has_next),
                ),
            ],
        ),
    ))
}

async fn club_aliases_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let repository = StorageRepository::new(pool);
    let aliases = repository.all_club_aliases().await?;
    let mut rows_html = String::new();
    for alias in aliases {
        let action = if alias.status == "active" {
            format!(
                "<form class=\"inline\" method=\"post\" action=\"/club-aliases/{}/deactivate\"><button type=\"submit\">Deaktivieren</button></form>",
                alias.id
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td><a href=\"/clubs/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            alias.id,
            escape_html(&alias.alias),
            alias.club_id,
            escape_html(&alias.canonical_name),
            escape_optional(alias.association_code.as_deref()),
            escape_optional(alias.source.as_deref()),
            escape_html(&alias.status),
            action
        );
    }
    let club_options = datalist_options(&ApplicationService::new(pool).club_names().await?);
    Ok(render_page(
        "Vereinsaliase",
        "club-aliases",
        render_template(
            CLUB_ALIASES_TEMPLATE,
            &[
                ("club_options", club_options),
                ("rows", empty_rows(rows_html, 7)),
            ],
        ),
    ))
}

fn honors_page() -> String {
    render_page("Ehrungen", "honors", render_template(HONORS_TEMPLATE, &[]))
}

fn combined_rows_html(rows: &[CombinedEvaluationRow]) -> String {
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td></tr>",
            detail_link("/athletes", row.athlete_id, row.athlete_name.as_deref()),
            detail_link("/clubs", row.club_id, row.club_name.as_deref()),
            row.year,
            escape_optional(row.lm_discipline.as_deref()),
            escape_optional(row.lm_event_class.as_deref()),
            result_kind_label(&row.lm_result_kind),
            row.lm_rank
                .map_or_else(String::new, |rank| rank.to_string()),
            escape_optional(row.lm_medal.as_deref()),
            yes_no(row.has_dm_participation == 1)
        );
    }
    rows_html
}

fn team_rows_html(rows: &[TeamRow]) -> String {
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\"><a href=\"/teams/{}\">{}</a></td><td><a href=\"/teams/{}\">{}</a></td><td>{}</td><td>{}</td><td>{} {}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td></tr>",
            row.id,
            row.id,
            row.id,
            escape_html(&row.canonical_name),
            detail_link("/clubs", row.club_id, row.club_name.as_deref()),
            escape_optional(row.association_code.as_deref()),
            escape_html(&row.competition_scope),
            row.competition_year,
            escape_optional(row.discipline.as_deref()),
            escape_optional(row.event_class.as_deref()),
            row.rank.map_or_else(String::new, |rank| rank.to_string()),
            row.score.map_or_else(String::new, format_score),
            row.member_count,
            source_detail_link(row.source_document_id, row.source_url.as_deref())
        );
    }
    rows_html
}

fn team_member_rows_html(rows: &[TeamMemberRow]) -> String {
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td></tr>",
            row.member_order,
            detail_link("/athletes", row.athlete_id, row.athlete_name.as_deref()),
            escape_html(&row.display_name),
            escape_optional(row.raw_name.as_deref()),
            row.score.map_or_else(String::new, format_score),
            escape_optional(row.medal.as_deref()),
            row.result_id.map_or_else(String::new, |id| id.to_string())
        );
    }
    rows_html
}

fn result_rows_html(rows: &[ResultRow]) -> String {
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td>{}</td><td>{} {}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td></tr>",
            row.id,
            detail_link("/athletes", row.athlete_id, row.athlete_name.as_deref()),
            detail_link("/clubs", row.club_id, row.club_name.as_deref()),
            escape_html(&row.competition_scope),
            row.competition_year,
            escape_html(&row.competition_name),
            escape_optional(row.discipline.as_deref()),
            row.rank.map_or_else(String::new, |rank| rank.to_string()),
            row.score.map_or_else(String::new, format_score),
            result_type_label(row),
            source_detail_link(row.source_document_id, row.source_url.as_deref())
        );
    }
    rows_html
}

fn render_page(title: &str, active: &str, content: String) -> String {
    render_template(
        LAYOUT_TEMPLATE,
        &[
            ("title", escape_html(title)),
            ("active_import_runs", active_class(active, "import-runs")),
            ("active_results", active_class(active, "results")),
            ("active_athletes", active_class(active, "athletes")),
            ("active_clubs", active_class(active, "clubs")),
            ("active_teams", active_class(active, "teams")),
            ("active_sources", active_class(active, "sources")),
            ("active_parser_runs", active_class(active, "parser-runs")),
            ("active_combined", active_class(active, "combined")),
            ("active_club_aliases", active_class(active, "club-aliases")),
            ("active_corrections", active_class(active, "corrections")),
            ("active_honors", active_class(active, "honors")),
            ("content", content),
        ],
    )
}

fn result_filters(query: &BTreeMap<String, String>, page: PageState) -> ResultFilters {
    ResultFilters {
        search: non_empty_query(query, "q"),
        year: query_i64(query, "year"),
        scope: non_empty_query(query, "scope"),
        association_code: non_empty_query(query, "kreis"),
        result_kind: non_empty_query(query, "wertung"),
        page: PageParams {
            limit: page_fetch_limit(page),
            offset: page_offset(page),
        },
        ..ResultFilters::default()
    }
}

fn athlete_filters(query: &BTreeMap<String, String>, page: PageState) -> AthleteFilters {
    AthleteFilters {
        search: non_empty_query(query, "q"),
        club: non_empty_query(query, "verein"),
        year: query_i64(query, "year"),
        page: PageParams {
            limit: page_fetch_limit(page),
            offset: page_offset(page),
        },
    }
}

fn club_filters(query: &BTreeMap<String, String>, page: PageState) -> ClubFilters {
    ClubFilters {
        search: non_empty_query(query, "q"),
        association_code: non_empty_query(query, "kreis"),
        year: query_i64(query, "year"),
        page: PageParams {
            limit: page_fetch_limit(page),
            offset: page_offset(page),
        },
    }
}

fn team_filters(query: &BTreeMap<String, String>, page: PageState) -> TeamFilters {
    TeamFilters {
        search: non_empty_query(query, "q"),
        year: query_i64(query, "year"),
        scope: non_empty_query(query, "scope"),
        association_code: non_empty_query(query, "kreis"),
        page: PageParams {
            limit: page_fetch_limit(page),
            offset: page_offset(page),
        },
    }
}

fn result_filter_form(filters: &ResultFilters, action: &str, page: PageState) -> String {
    format!(
        r#"<form class="filter-bar" method="get" action="{action}">
  <label>Suche <input name="q" value="{search}"></label>
  <label>Jahr <input name="year" inputmode="numeric" value="{year}"></label>
  <label>Ursprung <input name="scope" value="{scope}" placeholder="LM, DM, KM"></label>
  <label>Kreis <input name="kreis" value="{kreis}" placeholder="OD"></label>
  <label>Wertung <select name="wertung">
    <option value="">Alle</option>
    <option value="individual" {individual_selected}>Einzel</option>
    <option value="team" {team_selected}>Mannschaft</option>
  </select></label>
  <input type="hidden" name="page_size" value="{page_size}">
  <div class="actions"><button type="submit">Filtern</button><a href="{action}">Zuruecksetzen</a></div>
</form>"#,
        search = escape_html(filters.search.as_deref().unwrap_or_default()),
        year = filters
            .year
            .map_or_else(String::new, |year| year.to_string()),
        scope = escape_html(filters.scope.as_deref().unwrap_or_default()),
        kreis = escape_html(filters.association_code.as_deref().unwrap_or_default()),
        individual_selected = selected_str(filters.result_kind.as_deref(), "individual"),
        team_selected = selected_str(filters.result_kind.as_deref(), "team"),
        page_size = page.page_size,
    )
}

fn athlete_filter_form(filters: &AthleteFilters, action: &str, page: PageState) -> String {
    format!(
        r#"<form class="filter-bar" method="get" action="{action}">
  <label>Suche <input name="q" value="{search}"></label>
  <label>Verein <input name="verein" value="{club}"></label>
  <label>Jahr <input name="year" inputmode="numeric" value="{year}"></label>
  <input type="hidden" name="page_size" value="{page_size}">
  <div class="actions"><button type="submit">Filtern</button><a href="{action}">Zuruecksetzen</a></div>
</form>"#,
        search = escape_html(filters.search.as_deref().unwrap_or_default()),
        club = escape_html(filters.club.as_deref().unwrap_or_default()),
        year = filters
            .year
            .map_or_else(String::new, |year| year.to_string()),
        page_size = page.page_size,
    )
}

fn club_filter_form(filters: &ClubFilters, action: &str, page: PageState) -> String {
    format!(
        r#"<form class="filter-bar" method="get" action="{action}">
  <label>Suche <input name="q" value="{search}"></label>
  <label>Kreis <input name="kreis" value="{kreis}" placeholder="OD"></label>
  <label>Jahr <input name="year" inputmode="numeric" value="{year}"></label>
  <input type="hidden" name="page_size" value="{page_size}">
  <div class="actions"><button type="submit">Filtern</button><a href="{action}">Zuruecksetzen</a></div>
</form>"#,
        search = escape_html(filters.search.as_deref().unwrap_or_default()),
        kreis = escape_html(filters.association_code.as_deref().unwrap_or_default()),
        year = filters
            .year
            .map_or_else(String::new, |year| year.to_string()),
        page_size = page.page_size,
    )
}

fn team_filter_form(filters: &TeamFilters, action: &str, page: PageState) -> String {
    format!(
        r#"<form class="filter-bar" method="get" action="{action}">
  <label>Suche <input name="q" value="{search}"></label>
  <label>Jahr <input name="year" inputmode="numeric" value="{year}"></label>
  <label>Ursprung <input name="scope" value="{scope}" placeholder="LM, DM, KM"></label>
  <label>Kreis <input name="kreis" value="{kreis}" placeholder="OD"></label>
  <input type="hidden" name="page_size" value="{page_size}">
  <div class="actions"><button type="submit">Filtern</button><a href="{action}">Zuruecksetzen</a></div>
</form>"#,
        search = escape_html(filters.search.as_deref().unwrap_or_default()),
        year = filters
            .year
            .map_or_else(String::new, |year| year.to_string()),
        scope = escape_html(filters.scope.as_deref().unwrap_or_default()),
        kreis = escape_html(filters.association_code.as_deref().unwrap_or_default()),
        page_size = page.page_size,
    )
}

fn combined_filter_form(filters: &ResultFilters, action: &str, page: PageState) -> String {
    format!(
        r#"<form class="filter-bar" method="get" action="{action}">
  <label>Suche <input name="q" value="{search}"></label>
  <label>Jahr <input name="year" inputmode="numeric" value="{year}"></label>
  <label>Verein <input name="verein" value="{club}"></label>
  <input type="hidden" name="page_size" value="{page_size}">
  <div class="actions"><button type="submit">Filtern</button><a href="{action}">Zuruecksetzen</a></div>
</form>"#,
        search = escape_html(filters.search.as_deref().unwrap_or_default()),
        year = filters
            .year
            .map_or_else(String::new, |year| year.to_string()),
        club = escape_html(filters.association_code.as_deref().unwrap_or_default()),
        page_size = page.page_size,
    )
}

fn page_state(query: &BTreeMap<String, String>) -> PageState {
    let page = query_usize(query, "page")
        .filter(|page| *page > 0)
        .unwrap_or(1);
    let page_size = query_usize(query, "page_size")
        .filter(|page_size| PAGE_SIZE_OPTIONS.contains(page_size))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    PageState { page, page_size }
}

fn page_fetch_limit(page: PageState) -> i64 {
    i64::try_from(page.page_size.saturating_add(1)).unwrap_or(1_001)
}

fn page_offset(page: PageState) -> i64 {
    let offset = page.page.saturating_sub(1).saturating_mul(page.page_size);
    i64::try_from(offset).unwrap_or(i64::MAX)
}

fn trim_page_rows<T>(rows: &mut Vec<T>, page: PageState) -> bool {
    if rows.len() > page.page_size {
        rows.truncate(page.page_size);
        true
    } else {
        false
    }
}

fn pagination_controls(
    action: &str,
    query: &BTreeMap<String, String>,
    page: PageState,
    item_count: usize,
    has_next: bool,
) -> String {
    let first_item = if item_count == 0 {
        0
    } else {
        page.page
            .saturating_sub(1)
            .saturating_mul(page.page_size)
            .saturating_add(1)
    };
    let last_item = first_item.saturating_add(item_count.saturating_sub(1));
    let previous = if page.page > 1 {
        format!(
            "<a href=\"{}\">Zurueck</a>",
            page_url(action, query, page.page - 1, page.page_size)
        )
    } else {
        "<span class=\"muted\">Zurueck</span>".to_string()
    };
    let next = if has_next {
        format!(
            "<a href=\"{}\">Weiter</a>",
            page_url(action, query, page.page + 1, page.page_size)
        )
    } else {
        "<span class=\"muted\">Weiter</span>".to_string()
    };
    format!(
        r#"<div class="pager">
  <div>{previous} <span>Seite {page_number}, Eintraege {first_item}-{last_item}</span> {next}</div>
  <form method="get" action="{action}">
    {hidden_inputs}
    <label>Seitengroesse <select name="page_size" onchange="this.form.submit()">{page_size_options}</select></label>
  </form>
</div>"#,
        page_number = page.page,
        hidden_inputs = pagination_hidden_inputs(query),
        page_size_options = page_size_options(page.page_size),
    )
}

fn page_url(
    action: &str,
    query: &BTreeMap<String, String>,
    page: usize,
    page_size: usize,
) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in query {
        if !matches!(key.as_str(), "page" | "page_size") && !value.trim().is_empty() {
            serializer.append_pair(key, value);
        }
    }
    serializer.append_pair("page", &page.to_string());
    serializer.append_pair("page_size", &page_size.to_string());
    let query = serializer.finish();
    format!("{action}?{query}")
}

fn pagination_hidden_inputs(query: &BTreeMap<String, String>) -> String {
    let mut inputs = String::from("<input type=\"hidden\" name=\"page\" value=\"1\">");
    for (key, value) in query {
        if !matches!(key.as_str(), "page" | "page_size") && !value.trim().is_empty() {
            let _ = write!(
                inputs,
                "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
                escape_html(key),
                escape_html(value)
            );
        }
    }
    inputs
}

fn page_size_options(selected: usize) -> String {
    let mut options = String::new();
    for size in PAGE_SIZE_OPTIONS {
        let _ = write!(
            options,
            "<option value=\"{size}\" {}>{size}</option>",
            if *size == selected { "selected" } else { "" }
        );
    }
    options
}

fn datalist_options(values: &[String]) -> String {
    let mut options = String::new();
    for value in values {
        let _ = writeln!(
            options,
            "<option value=\"{}\"></option>",
            escape_html(value)
        );
    }
    options
}

fn non_empty_query(query: &BTreeMap<String, String>, key: &str) -> Option<String> {
    query
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn query_i64(query: &BTreeMap<String, String>, key: &str) -> Option<i64> {
    non_empty_query(query, key).and_then(|value| value.parse::<i64>().ok())
}

fn query_usize(query: &BTreeMap<String, String>, key: &str) -> Option<usize> {
    non_empty_query(query, key).and_then(|value| value.parse::<usize>().ok())
}

fn selected_str(value: Option<&str>, expected: &str) -> &'static str {
    if value.is_some_and(|value| value == expected) {
        "selected"
    } else {
        ""
    }
}

async fn create_manual_override(pool: &sqlx::SqlitePool, body: &str) -> Result<()> {
    let form = parse_form_urlencoded(body);
    let entity_type = form_value(&form, "entity_type")?;
    let old_value = form_value(&form, "old_value")?;
    let new_value = form_value(&form, "new_value")?;
    let reason = form
        .get("reason")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if !matches!(entity_type.as_str(), "club" | "athlete") {
        anyhow::bail!("unknown correction type {entity_type}");
    }

    let repository = StorageRepository::new(pool);
    repository
        .upsert_manual_override(&NewManualOverride {
            scope: "global".to_string(),
            entity_type,
            entity_id: None,
            source_document_id: None,
            parsed_result_row_id: None,
            field_name: "canonical_name".to_string(),
            old_value,
            new_value,
            reason,
            status: "active".to_string(),
        })
        .await?;
    Ok(())
}

async fn create_club_alias(pool: &sqlx::SqlitePool, body: &str) -> Result<()> {
    let form = parse_form_urlencoded(body);
    let alias = form_value(&form, "alias")?;
    let club_name = form_value(&form, "club")?;
    let association_code = non_empty_query(&form, "association_code");
    let source = non_empty_query(&form, "source").or_else(|| Some("web".to_owned()));
    let repository = StorageRepository::new(pool);
    let club_id = if let Some(club_id) = repository.find_club_id_by_name(&club_name).await? {
        club_id
    } else {
        repository
            .upsert_club(&NewClub {
                canonical_name: club_name,
                association_code: association_code.clone(),
                source: Some("web-alias".to_owned()),
            })
            .await?
    };
    repository
        .upsert_club_alias(&NewClubAlias {
            club_id,
            alias,
            association_code,
            source,
            status: "active".to_owned(),
        })
        .await?;
    Ok(())
}

async fn deactivate_club_alias(pool: &sqlx::SqlitePool, id: i64) -> Result<()> {
    StorageRepository::new(pool).deactivate_club_alias(id).await
}

async fn revoke_manual_override(pool: &sqlx::SqlitePool, id: i64) -> Result<()> {
    sqlx::query(
        r"
        UPDATE manual_overrides
        SET status = 'revoked', updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        ",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("could not revoke manual override")?;
    Ok(())
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let request = read_request(stream)?;
    let Some((method, path)) = parse_request_line(&request) else {
        anyhow::bail!("invalid HTTP request line");
    };
    Ok(HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: request_query(&request),
        body: request_body(&request).to_string(),
    })
}

fn read_request(stream: &mut TcpStream) -> Result<String> {
    let mut buffer = [0; 4096];
    let length = stream.read(&mut buffer).context("could not read request")?;
    let mut request = String::from_utf8_lossy(&buffer[..length]).to_string();
    let content_length = content_length(&request);
    while request_body(&request).len() < content_length {
        let length = stream
            .read(&mut buffer)
            .context("could not read request body")?;
        if length == 0 {
            break;
        }
        request.push_str(&String::from_utf8_lossy(&buffer[..length]));
    }
    Ok(request)
}

fn parse_request_line(request: &str) -> Option<(&str, &str)> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?.split('?').next()?;
    Some((method, path))
}

fn request_query(request: &str) -> BTreeMap<String, String> {
    let Some(line) = request.lines().next() else {
        return BTreeMap::new();
    };
    let Some(target) = line.split_whitespace().nth(1) else {
        return BTreeMap::new();
    };
    let Some((_, query)) = target.split_once('?') else {
        return BTreeMap::new();
    };
    parse_form_urlencoded(query)
}

fn content_length(request: &str) -> usize {
    request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn request_body(request: &str) -> &str {
    request.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn write_response(stream: &mut TcpStream, response: WebResponse) -> Result<()> {
    let (status, headers, body) = match response {
        WebResponse::Html(body) => ("200 OK", String::new(), body),
        WebResponse::Redirect(location) => (
            "303 See Other",
            format!("Location: {location}\r\n"),
            String::new(),
        ),
        WebResponse::NotFound => (
            "404 Not Found",
            String::new(),
            render_page("Nicht gefunden", "", "<h1>Nicht gefunden</h1>".to_string()),
        ),
        WebResponse::MethodNotAllowed => (
            "405 Method Not Allowed",
            String::new(),
            "Method not allowed".to_string(),
        ),
        WebResponse::InternalError(error) => (
            "500 Internal Server Error",
            String::new(),
            render_page(
                "Fehler",
                "",
                format!("<h1>Fehler</h1><p>{}</p>", escape_html(&error)),
            ),
        ),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .context("could not write response")
}

fn active_class(active: &str, item: &str) -> String {
    if active == item {
        "active".to_string()
    } else {
        String::new()
    }
}

fn selected_attr(value: Option<&String>, expected: &str) -> String {
    if value.is_some_and(|value| value == expected) {
        "selected".to_string()
    } else {
        String::new()
    }
}

fn empty_rows(rows: String, column_count: usize) -> String {
    if rows.trim().is_empty() {
        format!(
            "<tr><td colspan=\"{column_count}\" class=\"empty\">Keine Daten vorhanden.</td></tr>"
        )
    } else {
        rows
    }
}

fn format_time_range(started_at: &str, finished_at: Option<&str>) -> String {
    finished_at.map_or_else(
        || started_at.to_string(),
        |finished_at| format!("{started_at} bis {finished_at}"),
    )
}

fn format_score(score: f64) -> String {
    let formatted = format!("{score:.1}");
    formatted.trim_end_matches(".0").to_string()
}

fn parse_form_urlencoded(body: &str) -> BTreeMap<String, String> {
    body.split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((decode_form_value(key), decode_form_value(value)))
        })
        .collect()
}

fn decode_form_value(value: &str) -> String {
    percent_decode_str(&value.replace('+', " "))
        .decode_utf8_lossy()
        .to_string()
}

fn form_value(form: &BTreeMap<String, String>, key: &str) -> Result<String> {
    form.get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .with_context(|| format!("missing form value {key}"))
}

fn parser_label(name: Option<&str>, version: Option<&str>) -> String {
    match (name, version) {
        (Some(name), Some(version)) if !version.is_empty() => {
            format!("{} {}", escape_html(name), escape_html(version))
        }
        (Some(name), _) => escape_html(name),
        _ => String::new(),
    }
}

fn entity_type_label(entity_type: &str) -> &'static str {
    match entity_type {
        "club" => "Verein",
        "athlete" => "Sportler",
        _ => "Unbekannt",
    }
}

fn result_type_label(row: &ResultRow) -> String {
    if row.participation_only == 1 {
        "Teilnahme".to_string()
    } else if let Some(medal) = row.medal.as_deref().filter(|medal| !medal.is_empty()) {
        escape_html(medal)
    } else if row.result_kind == "team" {
        "Mannschaft".to_string()
    } else {
        escape_html(row.event_class.as_deref().unwrap_or("Einzel"))
    }
}

fn result_kind_label(kind: &str) -> &'static str {
    match kind {
        "team" => "Mannschaft",
        "individual" => "Einzel",
        _ => "Unbekannt",
    }
}

const fn yes_no(value: bool) -> &'static str {
    if value { "ja" } else { "nein" }
}

fn issue_name_cell(raw: Option<&str>, normalized: Option<&str>) -> String {
    match (raw, normalized) {
        (Some(raw), Some(normalized)) if raw != normalized => {
            format!(
                "{}<br><span class=\"muted\">{}</span>",
                escape_html(raw),
                escape_html(normalized)
            )
        }
        (Some(raw), _) => escape_html(raw),
        (_, Some(normalized)) => escape_html(normalized),
        _ => String::new(),
    }
}

fn discipline_issue_cell(row: &ParsedIssueRow) -> String {
    [
        row.discipline_code.as_deref(),
        row.raw_discipline.as_deref(),
        row.class_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .map(escape_html)
    .collect::<Vec<_>>()
    .join(" ")
}

fn correction_prefill_links(row: &ParsedIssueRow) -> String {
    let mut links = Vec::new();
    if let Some(raw_club) = row
        .raw_club_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        links.push(prefill_link("club", raw_club));
    }
    if let Some(raw_shooter) = row
        .raw_shooter_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        links.push(prefill_link("athlete", raw_shooter));
    }
    links.join(" ")
}

fn prefill_link(entity_type: &str, old_value: &str) -> String {
    format!(
        "<a href=\"/corrections?entity_type={}&old_value={}\">Korrigieren</a>",
        escape_html(entity_type),
        url_encode(old_value)
    )
}

fn source_link(source_url: Option<&str>) -> String {
    source_url
        .filter(|url| !url.is_empty())
        .map_or_else(String::new, |url| {
            format!("<a href=\"{}\">PDF</a>", escape_html(url))
        })
}

fn source_detail_link(source_document_id: Option<i64>, source_url: Option<&str>) -> String {
    source_document_id.map_or_else(
        || source_link(source_url),
        |id| format!("<a href=\"/sources/{id}\">PDF</a>"),
    )
}

fn detail_link(base_path: &str, id: Option<i64>, label: Option<&str>) -> String {
    match (id, label) {
        (Some(id), Some(label)) if !label.is_empty() => {
            format!("<a href=\"{base_path}/{id}\">{}</a>", escape_html(label))
        }
        (_, Some(label)) => escape_html(label),
        _ => String::new(),
    }
}

fn url_encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn escape_optional(value: Option<&str>) -> String {
    value.map_or_else(String::new, escape_html)
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
    use super::{empty_rows, escape_html, format_score, parse_request_line};

    #[test]
    fn parses_get_request_path_without_query() {
        assert_eq!(
            parse_request_line("GET /import-runs?x=1 HTTP/1.1\r\n\r\n"),
            Some(("GET", "/import-runs"))
        );
    }

    #[test]
    fn escapes_html_control_characters() {
        assert_eq!(escape_html("<script>&\"\n"), "&lt;script&gt;&amp;&quot; ");
    }

    #[test]
    fn formats_scores_without_trailing_zero() {
        assert_eq!(format_score(399.0), "399");
        assert_eq!(format_score(399.5), "399.5");
    }

    #[test]
    fn renders_empty_table_row() {
        assert!(empty_rows(String::new(), 3).contains("colspan=\"3\""));
    }
}
