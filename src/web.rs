use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::storage::{Database, DatabaseConfig};
use crate::template::render_template;

const LAYOUT_TEMPLATE: &str = include_str!("../templates/web-layout.html");
const IMPORT_RUNS_TEMPLATE: &str = include_str!("../templates/web-import-runs.html");
const IMPORT_RUN_RESULTS_TEMPLATE: &str = include_str!("../templates/web-import-run-results.html");
const RESULTS_TEMPLATE: &str = include_str!("../templates/web-results.html");
const ATHLETES_TEMPLATE: &str = include_str!("../templates/web-athletes.html");
const CLUBS_TEMPLATE: &str = include_str!("../templates/web-clubs.html");
const HONORS_TEMPLATE: &str = include_str!("../templates/web-honors.html");

#[derive(Debug, Clone)]
pub struct WebConfig {
    pub database_path: PathBuf,
    pub bind: String,
}

#[derive(Debug, Clone)]
pub struct WebServer {
    config: WebConfig,
}

#[derive(Debug, sqlx::FromRow)]
struct ImportRunRow {
    id: i64,
    source_name: String,
    run_kind: String,
    parser_name: Option<String>,
    parser_version: Option<String>,
    input_path: Option<String>,
    status: String,
    started_at: String,
    finished_at: Option<String>,
    result_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ResultRow {
    id: i64,
    athlete_name: Option<String>,
    club_name: Option<String>,
    discipline: Option<String>,
    competition_name: String,
    competition_scope: String,
    competition_year: i64,
    result_kind: String,
    rank: Option<i64>,
    score: Option<f64>,
    medal: Option<String>,
    participation_only: i64,
    event_class: Option<String>,
    source_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct AthleteRow {
    id: i64,
    canonical_name: String,
    result_count: i64,
    club_count: i64,
    latest_year: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ClubRow {
    id: i64,
    canonical_name: String,
    association_code: Option<String>,
    result_count: i64,
    athlete_count: i64,
    latest_year: Option<i64>,
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
    let Ok(request) = read_request(stream) else {
        return WebResponse::MethodNotAllowed;
    };
    let Some((method, path)) = parse_request_line(&request) else {
        return WebResponse::NotFound;
    };
    if method != "GET" {
        return WebResponse::MethodNotAllowed;
    }

    match route(path, pool).await {
        Ok(response) => response,
        Err(error) => WebResponse::InternalError(error.to_string()),
    }
}

async fn route(path: &str, pool: &sqlx::SqlitePool) -> Result<WebResponse> {
    match path {
        "/" => Ok(WebResponse::Redirect("/import-runs")),
        "/import-runs" => import_runs_page(pool).await.map(WebResponse::Html),
        "/results" => results_page(pool).await.map(WebResponse::Html),
        "/athletes" => athletes_page(pool).await.map(WebResponse::Html),
        "/clubs" => clubs_page(pool).await.map(WebResponse::Html),
        "/honors" => Ok(WebResponse::Html(honors_page())),
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

async fn import_runs_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = sqlx::query_as::<_, ImportRunRow>(
        r"
        SELECT
            import_runs.id,
            import_runs.source_name,
            import_runs.run_kind,
            import_runs.parser_name,
            import_runs.parser_version,
            import_runs.input_path,
            import_runs.status,
            import_runs.started_at,
            import_runs.finished_at,
            COUNT(results.id) AS result_count
        FROM import_runs
        LEFT JOIN results ON results.import_run_id = import_runs.id
        GROUP BY import_runs.id
        ORDER BY import_runs.started_at DESC, import_runs.id DESC
        ",
    )
    .fetch_all(pool)
    .await
    .context("could not load import runs")?;

    let mut rows_html = String::new();
    for row in rows {
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
    let rows = result_rows(pool, Some(import_run_id)).await?;
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

async fn results_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = result_rows(pool, None).await?;
    Ok(render_page(
        "Ergebnisse",
        "results",
        render_template(
            RESULTS_TEMPLATE,
            &[("rows", empty_rows(result_rows_html(&rows), 10))],
        ),
    ))
}

async fn athletes_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = sqlx::query_as::<_, AthleteRow>(
        r"
        SELECT
            athletes.id,
            athletes.canonical_name,
            COUNT(results.id) AS result_count,
            COUNT(DISTINCT results.club_id) AS club_count,
            MAX(competitions.year) AS latest_year
        FROM athletes
        LEFT JOIN results ON results.athlete_id = athletes.id
        LEFT JOIN competitions ON competitions.id = results.competition_id
        GROUP BY athletes.id
        ORDER BY athletes.canonical_name
        ",
    )
    .fetch_all(pool)
    .await
    .context("could not load athletes")?;

    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
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
        render_template(ATHLETES_TEMPLATE, &[("rows", empty_rows(rows_html, 5))]),
    ))
}

async fn clubs_page(pool: &sqlx::SqlitePool) -> Result<String> {
    let rows = sqlx::query_as::<_, ClubRow>(
        r"
        SELECT
            clubs.id,
            clubs.canonical_name,
            clubs.association_code,
            COUNT(results.id) AS result_count,
            COUNT(DISTINCT results.athlete_id) AS athlete_count,
            MAX(competitions.year) AS latest_year
        FROM clubs
        LEFT JOIN results ON results.club_id = clubs.id
        LEFT JOIN competitions ON competitions.id = results.competition_id
        GROUP BY clubs.id
        ORDER BY clubs.canonical_name
        ",
    )
    .fetch_all(pool)
    .await
    .context("could not load clubs")?;

    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
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
        render_template(CLUBS_TEMPLATE, &[("rows", empty_rows(rows_html, 6))]),
    ))
}

fn honors_page() -> String {
    render_page("Ehrungen", "honors", render_template(HONORS_TEMPLATE, &[]))
}

async fn result_rows(
    pool: &sqlx::SqlitePool,
    import_run_id: Option<i64>,
) -> Result<Vec<ResultRow>> {
    if let Some(import_run_id) = import_run_id {
        sqlx::query_as::<_, ResultRow>(
            r"
            SELECT
                results.id,
                athletes.canonical_name AS athlete_name,
                clubs.canonical_name AS club_name,
                TRIM(COALESCE(NULLIF(disciplines.code, ''), '') || ' ' || COALESCE(disciplines.name, '')) AS discipline,
                competitions.name AS competition_name,
                competitions.scope AS competition_scope,
                competitions.year AS competition_year,
                results.result_kind,
                results.rank,
                results.score,
                results.medal,
                results.participation_only,
                results.event_class,
                source_documents.url AS source_url
            FROM results
            JOIN competitions ON competitions.id = results.competition_id
            LEFT JOIN athletes ON athletes.id = results.athlete_id
            LEFT JOIN clubs ON clubs.id = results.club_id
            LEFT JOIN disciplines ON disciplines.id = results.discipline_id
            LEFT JOIN source_documents ON source_documents.id = results.source_document_id
            WHERE results.import_run_id = ?
            ORDER BY competitions.year DESC, competitions.scope, results.rank, athlete_name
            ",
        )
        .bind(import_run_id)
        .fetch_all(pool)
        .await
        .context("could not load results")
    } else {
        sqlx::query_as::<_, ResultRow>(
            r"
            SELECT
                results.id,
                athletes.canonical_name AS athlete_name,
                clubs.canonical_name AS club_name,
                TRIM(COALESCE(NULLIF(disciplines.code, ''), '') || ' ' || COALESCE(disciplines.name, '')) AS discipline,
                competitions.name AS competition_name,
                competitions.scope AS competition_scope,
                competitions.year AS competition_year,
                results.result_kind,
                results.rank,
                results.score,
                results.medal,
                results.participation_only,
                results.event_class,
                source_documents.url AS source_url
            FROM results
            JOIN competitions ON competitions.id = results.competition_id
            LEFT JOIN athletes ON athletes.id = results.athlete_id
            LEFT JOIN clubs ON clubs.id = results.club_id
            LEFT JOIN disciplines ON disciplines.id = results.discipline_id
            LEFT JOIN source_documents ON source_documents.id = results.source_document_id
            ORDER BY competitions.year DESC, competitions.scope, results.rank, athlete_name
            ",
        )
        .fetch_all(pool)
        .await
        .context("could not load results")
    }
}

fn result_rows_html(rows: &[ResultRow]) -> String {
    let mut rows_html = String::new();
    for row in rows {
        let _ = writeln!(
            rows_html,
            "<tr><td class=\"num\">{}</td><td>{}</td><td>{}</td><td>{} {}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td></tr>",
            row.id,
            escape_optional(row.athlete_name.as_deref()),
            escape_optional(row.club_name.as_deref()),
            escape_html(&row.competition_scope),
            row.competition_year,
            escape_html(&row.competition_name),
            escape_optional(row.discipline.as_deref()),
            row.rank.map_or_else(String::new, |rank| rank.to_string()),
            row.score.map_or_else(String::new, format_score),
            result_type_label(row),
            source_link(row.source_url.as_deref())
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
            ("active_honors", active_class(active, "honors")),
            ("content", content),
        ],
    )
}

fn read_request(stream: &mut TcpStream) -> Result<String> {
    let mut buffer = [0; 4096];
    let length = stream.read(&mut buffer).context("could not read request")?;
    Ok(String::from_utf8_lossy(&buffer[..length]).to_string())
}

fn parse_request_line(request: &str) -> Option<(&str, &str)> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?.split('?').next()?;
    Some((method, path))
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

fn parser_label(name: Option<&str>, version: Option<&str>) -> String {
    match (name, version) {
        (Some(name), Some(version)) if !version.is_empty() => {
            format!("{} {}", escape_html(name), escape_html(version))
        }
        (Some(name), _) => escape_html(name),
        _ => String::new(),
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

fn source_link(source_url: Option<&str>) -> String {
    source_url
        .filter(|url| !url.is_empty())
        .map_or_else(String::new, |url| {
            format!("<a href=\"{}\">PDF</a>", escape_html(url))
        })
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
