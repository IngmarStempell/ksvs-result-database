use anyhow::{Context, Result, ensure};
use sqlx::SqliteConnection;

use super::{AthleteAlias, NewAthleteAlias, StorageRepository};

impl StorageRepository<'_> {
    /// Stores an active alias for an athlete.
    ///
    /// # Errors
    /// Returns an error when the alias cannot be stored.
    pub async fn upsert_athlete_alias(&self, alias: &NewAthleteAlias) -> Result<i64> {
        ensure!(
            !alias.alias.trim().is_empty(),
            "Bitte einen Alias eingeben."
        );
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO athlete_aliases (athlete_id, alias, source, status) VALUES (?, ?, ?, ?) ON CONFLICT(alias) WHERE status = 'active' DO UPDATE SET athlete_id = excluded.athlete_id, source = excluded.source RETURNING id",
        )
        .bind(alias.athlete_id)
        .bind(alias.alias.trim())
        .bind(&alias.source)
        .bind(&alias.status)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    /// Lists all athlete aliases.
    ///
    /// # Errors
    /// Returns an error when the aliases cannot be loaded.
    pub async fn all_athlete_aliases(&self) -> Result<Vec<AthleteAlias>> {
        sqlx::query_as::<_, AthleteAlias>(
            "SELECT athlete_aliases.id, athlete_aliases.athlete_id, athlete_aliases.alias, athletes.canonical_name, athlete_aliases.source, athlete_aliases.status FROM athlete_aliases JOIN athletes ON athletes.id = athlete_aliases.athlete_id ORDER BY athletes.canonical_name, athlete_aliases.alias",
        )
        .fetch_all(self.pool)
        .await
        .context("could not load athlete aliases")
    }

    /// Lists active athlete aliases for import normalization.
    ///
    /// # Errors
    /// Returns an error when aliases cannot be loaded.
    pub async fn active_athlete_aliases(&self) -> Result<Vec<AthleteAlias>> {
        Ok(self
            .all_athlete_aliases()
            .await?
            .into_iter()
            .filter(|alias| alias.status == "active")
            .collect())
    }

    /// Deactivates an athlete alias.
    ///
    /// # Errors
    /// Returns an error when the alias does not exist or the update fails.
    pub async fn deactivate_athlete_alias(&self, id: i64) -> Result<()> {
        let affected = sqlx::query("UPDATE athlete_aliases SET status = 'inactive', updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'active'")
            .bind(id).execute(self.pool).await?.rows_affected();
        ensure!(
            affected == 1,
            "Der aktive Sportleralias existiert nicht mehr."
        );
        Ok(())
    }

    /// Merges a source athlete into an existing target athlete.
    ///
    /// # Errors
    /// Returns an error for invalid IDs or database failures.
    pub async fn merge_athlete(&self, source_id: i64, target_id: i64) -> Result<()> {
        ensure!(
            source_id != target_id,
            "Quell- und Zielsportler müssen unterschiedlich sein."
        );
        let mut tx = self.pool.begin().await?;
        let source_name = athlete_name(&mut tx, source_id).await?;
        athlete_name(&mut tx, target_id).await?;
        sqlx::query("UPDATE results SET athlete_id = ? WHERE athlete_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE team_members SET athlete_id = ? WHERE athlete_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE team_result_members SET athlete_id = ? WHERE athlete_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT OR IGNORE INTO athlete_aliases (athlete_id, alias, source, status) SELECT ?, alias, COALESCE(source, 'web-merge'), status FROM athlete_aliases WHERE athlete_id = ?").bind(target_id).bind(source_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM athlete_aliases WHERE athlete_id = ? AND EXISTS (SELECT 1 FROM athlete_aliases target_alias WHERE target_alias.athlete_id = ? AND target_alias.alias = athlete_aliases.alias AND target_alias.id != athlete_aliases.id)").bind(source_id).bind(target_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE athlete_aliases SET athlete_id = ? WHERE athlete_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT OR IGNORE INTO athlete_aliases (athlete_id, alias, source, status) SELECT ?, ?, 'web-merge', 'active' WHERE NOT EXISTS (SELECT 1 FROM athlete_aliases WHERE alias = ? AND status = 'active')")
            .bind(target_id)
            .bind(&source_name)
            .bind(&source_name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO manual_overrides (scope, entity_type, entity_id, field_name, old_value, new_value, reason, status) SELECT 'entity', 'athlete', ?, 'merged_into', ?, canonical_name, 'Sportler zusammengeführt', 'applied' FROM athletes WHERE id = ?").bind(source_id).bind(&source_name).bind(target_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM athletes WHERE id = ?")
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn athlete_name(connection: &mut SqliteConnection, id: i64) -> Result<String> {
    sqlx::query_scalar("SELECT canonical_name FROM athletes WHERE id = ?")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .context("Sportler nicht gefunden.")
}
