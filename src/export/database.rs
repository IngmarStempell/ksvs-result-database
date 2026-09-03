use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;

use super::html::{render_combined_html, render_html_export};
use super::models::{
    CombinedClub, CombinedExport, DatabaseCombinedExportConfig, DatabasePodiumExportConfig,
    ParticipationMatch, PodiumExport, PodiumExportItem, PodiumResultKind,
};
use crate::storage::{Database, DatabaseConfig};

#[derive(Debug)]
pub struct DatabasePodiumExporter {
    config: DatabasePodiumExportConfig,
}

#[derive(Debug)]
pub struct DatabaseCombinedExporter {
    config: DatabaseCombinedExportConfig,
}

#[derive(Debug, sqlx::FromRow)]
struct DatabasePodiumRow {
    source_name: String,
    rank: i64,
    result_kind: String,
    shooter: String,
    club: String,
    canonical_club: String,
    association_code: String,
    association_name: String,
    discipline: Option<String>,
    discipline_code: Option<String>,
    class_name: Option<String>,
    event_name: String,
    event_date: Option<String>,
    score: Option<f64>,
    pdf_url: String,
    local_path: String,
}

impl TryFrom<DatabasePodiumRow> for PodiumExportItem {
    type Error = anyhow::Error;

    fn try_from(row: DatabasePodiumRow) -> Result<Self> {
        let result_kind = match row.result_kind.as_str() {
            "team" => PodiumResultKind::Team,
            _ => PodiumResultKind::Individual,
        };
        Ok(Self {
            source_name: row.source_name,
            rank: u32::try_from(row.rank).context("rank does not fit into u32")?,
            result_kind,
            shooter: row.shooter,
            club: row.club,
            canonical_club: row.canonical_club,
            association_code: row.association_code,
            association_name: row.association_name,
            discipline: row.discipline,
            discipline_code: row.discipline_code,
            class_name: row.class_name,
            event_name: row.event_name,
            event_date: row.event_date,
            score: row.score,
            pdf_url: row.pdf_url,
            local_path: Path::new(&row.local_path).to_path_buf(),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct DatabaseParticipationRow {
    source_name: String,
    shooter: Option<String>,
    club: String,
    canonical_club: String,
    pdf_url: String,
    local_path: String,
}

impl DatabasePodiumExporter {
    #[must_use]
    pub const fn new(config: DatabasePodiumExportConfig) -> Self {
        Self { config }
    }

    /// Creates a stable podium JSON and HTML export from canonical database results.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be read or output files cannot be written.
    pub async fn run(&self) -> Result<PodiumExport> {
        let database = Database::new(DatabaseConfig {
            path: self.config.database_path.clone(),
        });
        let pool = database.migrated_pool().await?;
        let rows = podium_rows_from_database(
            &pool,
            self.config.year,
            &self.config.competition_scope,
            &self.config.focus_association_code,
            self.config.max_place,
        )
        .await?;
        pool.close().await;

        let items = rows
            .into_iter()
            .map(PodiumExportItem::try_from)
            .collect::<Result<Vec<_>>>()?;
        let export = PodiumExport {
            generated_at: Utc::now(),
            source_report_path: self.config.database_path.clone(),
            source_name: self.config.competition_scope.clone(),
            focus_association_code: self.config.focus_association_code.clone(),
            max_place: self.config.max_place,
            item_count: items.len(),
            manual_review_count: 0,
            manual_review_pdfs: Vec::new(),
            items,
        };
        write_podium_json(&self.config.json_output_path, &export)?;
        write_podium_html(&self.config.html_output_path, &export)?;
        Ok(export)
    }
}

impl DatabaseCombinedExporter {
    #[must_use]
    pub const fn new(config: DatabaseCombinedExportConfig) -> Self {
        Self { config }
    }

    /// Creates a combined LM/DM JSON and HTML export from canonical database results.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be read or output files cannot be written.
    pub async fn run(&self) -> Result<CombinedExport> {
        let database = Database::new(DatabaseConfig {
            path: self.config.database_path.clone(),
        });
        let pool = database.migrated_pool().await?;
        let podium_rows = podium_rows_from_database(
            &pool,
            self.config.year,
            "LM",
            &self.config.focus_association_code,
            self.config.max_place,
        )
        .await?;
        let participation_rows = participation_rows_from_database(
            &pool,
            self.config.year,
            &self.config.focus_association_code,
        )
        .await?;
        pool.close().await;

        let podium_items = podium_rows
            .into_iter()
            .map(PodiumExportItem::try_from)
            .collect::<Result<Vec<_>>>()?;
        let participation_matches = participation_matches_from_rows(participation_rows);
        let export = combined_export_from_database(
            podium_items,
            participation_matches,
            &self.config.database_path,
            &self.config.focus_association_code,
        );
        write_combined_json(&self.config.json_output_path, &export)?;
        write_combined_html(&self.config.html_output_path, &export)?;
        Ok(export)
    }
}

async fn podium_rows_from_database(
    pool: &sqlx::SqlitePool,
    year: i64,
    scope: &str,
    focus_association_code: &str,
    max_place: u32,
) -> Result<Vec<DatabasePodiumRow>> {
    sqlx::query_as::<_, DatabasePodiumRow>(
        r"
        SELECT
            COALESCE(source_documents.source_name, competitions.scope) AS source_name,
            results.rank AS rank,
            results.result_kind AS result_kind,
            COALESCE(athletes.canonical_name, results.raw_shooter_name, '') AS shooter,
            COALESCE(results.raw_club_name, clubs.canonical_name, '') AS club,
            COALESCE(clubs.canonical_name, results.raw_club_name, '') AS canonical_club,
            COALESCE(clubs.association_code, '') AS association_code,
            COALESCE(clubs.association_code, '') AS association_name,
            disciplines.name AS discipline,
            NULLIF(disciplines.code, '') AS discipline_code,
            results.event_class AS class_name,
            competitions.name AS event_name,
            competitions.date_from AS event_date,
            results.score AS score,
            COALESCE(source_documents.url, '') AS pdf_url,
            COALESCE(source_documents.local_path, '') AS local_path
        FROM results
        JOIN competitions ON competitions.id = results.competition_id
        LEFT JOIN athletes ON athletes.id = results.athlete_id
        LEFT JOIN clubs ON clubs.id = results.club_id
        LEFT JOIN disciplines ON disciplines.id = results.discipline_id
        LEFT JOIN source_documents ON source_documents.id = results.source_document_id
        WHERE competitions.year = ?
            AND competitions.scope = ?
            AND results.participation_only = 0
            AND results.rank IS NOT NULL
            AND results.rank <= ?
            AND (? = 'all' OR COALESCE(clubs.association_code, '') = ?)
        ORDER BY shooter, canonical_club, results.rank, discipline
        ",
    )
    .bind(year)
    .bind(scope)
    .bind(i64::from(max_place))
    .bind(focus_association_code)
    .bind(focus_association_code)
    .fetch_all(pool)
    .await
    .context("could not read podium rows from database")
}

async fn participation_rows_from_database(
    pool: &sqlx::SqlitePool,
    year: i64,
    focus_association_code: &str,
) -> Result<Vec<DatabaseParticipationRow>> {
    sqlx::query_as::<_, DatabaseParticipationRow>(
        r"
        SELECT
            COALESCE(source_documents.source_name, competitions.scope) AS source_name,
            athletes.canonical_name AS shooter,
            COALESCE(results.raw_club_name, clubs.canonical_name, '') AS club,
            COALESCE(clubs.canonical_name, results.raw_club_name, '') AS canonical_club,
            COALESCE(source_documents.url, '') AS pdf_url,
            COALESCE(source_documents.local_path, '') AS local_path
        FROM results
        JOIN competitions ON competitions.id = results.competition_id
        LEFT JOIN athletes ON athletes.id = results.athlete_id
        LEFT JOIN clubs ON clubs.id = results.club_id
        LEFT JOIN source_documents ON source_documents.id = results.source_document_id
        WHERE competitions.year = ?
            AND competitions.scope = 'DM'
            AND results.participation_only = 1
            AND (? = 'all' OR COALESCE(clubs.association_code, '') = ?)
        ORDER BY canonical_club, pdf_url, shooter
        ",
    )
    .bind(year)
    .bind(focus_association_code)
    .bind(focus_association_code)
    .fetch_all(pool)
    .await
    .context("could not read participation rows from database")
}

fn participation_matches_from_rows(rows: Vec<DatabaseParticipationRow>) -> Vec<ParticipationMatch> {
    let mut matches = BTreeMap::<(String, String), ParticipationMatch>::new();
    for row in rows {
        let key = (row.canonical_club.clone(), row.pdf_url.clone());
        let entry = matches.entry(key).or_insert_with(|| ParticipationMatch {
            club: row.club,
            canonical_club: row.canonical_club,
            shooters: Vec::new(),
            source_name: row.source_name,
            pdf_url: row.pdf_url,
            local_path: Path::new(&row.local_path).to_path_buf(),
            text_char_count: 0,
        });
        if let Some(shooter) = row.shooter
            && !entry.shooters.contains(&shooter)
        {
            entry.shooters.push(shooter);
        }
    }
    matches.into_values().collect()
}

fn combined_export_from_database(
    podium_items: Vec<PodiumExportItem>,
    participation_matches: Vec<ParticipationMatch>,
    database_path: &Path,
    focus_association_code: &str,
) -> CombinedExport {
    let mut clubs = BTreeMap::<String, CombinedClub>::new();
    for item in podium_items {
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
    for item in participation_matches {
        let club = item.canonical_club.clone();
        clubs
            .entry(club.clone())
            .or_insert_with(|| CombinedClub {
                club,
                podium_items: Vec::new(),
                participation_matches: Vec::new(),
            })
            .participation_matches
            .push(item);
    }
    let clubs = clubs.into_values().collect::<Vec<_>>();
    let podium_item_count = clubs.iter().map(|club| club.podium_items.len()).sum();
    let participation_match_count = clubs
        .iter()
        .map(|club| club.participation_matches.len())
        .sum();
    CombinedExport {
        generated_at: Utc::now(),
        podium_export_path: database_path.to_path_buf(),
        participation_export_path: database_path.to_path_buf(),
        focus_association_code: focus_association_code.to_owned(),
        club_count: clubs.len(),
        podium_item_count,
        participation_match_count,
        clubs,
    }
}

fn write_podium_json(path: &Path, export: &PodiumExport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(export)?;
    fs::write(path, content).with_context(|| format!("could not write {}", path.display()))
}

fn write_podium_html(path: &Path, export: &PodiumExport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = render_html_export(export, path.parent());
    fs::write(path, content).with_context(|| format!("could not write {}", path.display()))
}

fn write_combined_json(path: &Path, export: &CombinedExport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(export)?;
    fs::write(path, content).with_context(|| format!("could not write {}", path.display()))
}

fn write_combined_html(path: &Path, export: &CombinedExport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = render_combined_html(export, path.parent());
    fs::write(path, content).with_context(|| format!("could not write {}", path.display()))
}
