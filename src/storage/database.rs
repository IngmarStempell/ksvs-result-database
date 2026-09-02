use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Database {
    config: DatabaseConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub database_path: PathBuf,
    pub applied_migration_count: i64,
}

impl Database {
    #[must_use]
    pub const fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Creates the `SQLite` database file and its parent directory if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created or `SQLite`
    /// cannot open the configured database file.
    pub async fn init(&self) -> Result<()> {
        create_parent_dir(&self.config.path)?;
        let pool = self.connect(true).await?;
        pool.close().await;
        Ok(())
    }

    /// Applies all pending embedded SQL migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or a migration fails.
    pub async fn migrate(&self) -> Result<MigrationReport> {
        self.init().await?;
        let pool = self.connect(true).await?;
        MIGRATOR
            .run(&pool)
            .await
            .with_context(|| format!("could not migrate {}", self.config.path.display()))?;
        let applied_migration_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE success = TRUE",
        )
        .fetch_one(&pool)
        .await
        .context("could not read migration state")?;
        pool.close().await;

        Ok(MigrationReport {
            database_path: self.config.path.clone(),
            applied_migration_count,
        })
    }

    async fn connect(&self, create_if_missing: bool) -> Result<sqlx::SqlitePool> {
        let options = SqliteConnectOptions::new()
            .filename(&self.config.path)
            .create_if_missing(create_if_missing)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);

        SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| {
                format!(
                    "could not open SQLite database {}",
                    self.config.path.display()
                )
            })
    }
}

fn create_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create database directory {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{Database, DatabaseConfig};

    #[tokio::test]
    async fn initializes_and_migrates_sqlite_database() {
        let path = std::env::temp_dir().join(format!(
            "pdf-explorer-storage-test-{}.sqlite",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is after unix epoch")
                .as_nanos()
        ));
        let database = Database::new(DatabaseConfig { path: path.clone() });

        let report = database.migrate().await.expect("database migrates");

        assert_eq!(report.database_path, path);
        assert!(report.applied_migration_count >= 1);
        let _ = std::fs::remove_file(&report.database_path);
        let _ = std::fs::remove_file(report.database_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(report.database_path.with_extension("sqlite-wal"));
    }
}
