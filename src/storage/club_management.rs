use anyhow::{Context, Result, ensure};
use sqlx::SqliteConnection;

use super::StorageRepository;

impl StorageRepository<'_> {
    /// Renames a club without changing its ID or parser data, retaining the old name as an alias.
    ///
    /// # Errors
    /// Returns an error for missing clubs, name conflicts or database failures.
    pub async fn rename_club(&self, club_id: i64, name: &str) -> Result<()> {
        let name = required_name(name)?;
        let mut tx = self.pool.begin().await?;
        let old_name = club_name(&mut tx, club_id).await?;
        if name == old_name {
            return Ok(());
        }
        ensure_name_available(&mut tx, club_id, name, None).await?;
        ensure_name_available(&mut tx, club_id, &old_name, None).await?;
        sqlx::query("UPDATE clubs SET canonical_name = ? WHERE id = ?")
            .bind(name)
            .bind(club_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO club_aliases (club_id, alias, association_code, source, status)
             SELECT id, ?, association_code, 'web-rename', 'active' FROM clubs WHERE id = ?
             ON CONFLICT(alias) WHERE status = 'active' DO NOTHING",
        )
        .bind(&old_name)
        .bind(club_id)
        .execute(&mut *tx)
        .await?;
        // Applied changes are audit records, not pending global import rules.
        sqlx::query(
            "INSERT INTO manual_overrides
             (scope, entity_type, entity_id, field_name, old_value, new_value, reason, status)
             VALUES ('entity', 'club', ?, 'canonical_name', ?, ?, 'Verein direkt umbenannt', 'applied')",
        ).bind(club_id).bind(&old_name).bind(name).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Creates or edits an active alias owned by the given club.
    ///
    /// # Errors
    /// Returns an error for invalid ownership, conflicting names or database failures.
    pub async fn save_club_alias(
        &self,
        club_id: i64,
        alias_id: Option<i64>,
        name: &str,
    ) -> Result<()> {
        let name = required_name(name)?;
        let mut tx = self.pool.begin().await?;
        club_name(&mut tx, club_id).await?;
        ensure_name_available(&mut tx, club_id, name, alias_id).await?;
        if let Some(id) = alias_id {
            let affected = sqlx::query(
                "UPDATE club_aliases SET alias = ?, updated_at = CURRENT_TIMESTAMP
                 WHERE id = ? AND club_id = ? AND status = 'active'",
            )
            .bind(name)
            .bind(id)
            .bind(club_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            ensure!(
                affected == 1,
                "Der aktive Alias gehört nicht zu diesem Verein oder existiert nicht mehr."
            );
        } else {
            sqlx::query(
                "INSERT INTO club_aliases (club_id, alias, association_code, source, status)
                 SELECT id, ?, association_code, 'web', 'active' FROM clubs WHERE id = ?
                 ON CONFLICT(alias) WHERE status = 'active' DO NOTHING",
            )
            .bind(name)
            .bind(club_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Deactivates an alias only if it belongs to the selected club.
    ///
    /// # Errors
    /// Returns an error for invalid ownership or database failures.
    pub async fn deactivate_owned_club_alias(&self, club_id: i64, alias_id: i64) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE club_aliases SET status = 'inactive', updated_at = CURRENT_TIMESTAMP
             WHERE id = ? AND club_id = ? AND status = 'active'",
        )
        .bind(alias_id)
        .bind(club_id)
        .execute(self.pool)
        .await?
        .rows_affected();
        ensure!(
            affected == 1,
            "Der aktive Alias gehört nicht zu diesem Verein oder existiert nicht mehr."
        );
        Ok(())
    }

    /// Merges a source club into an existing target club while preserving result history.
    ///
    /// # Errors
    /// Returns an error for invalid IDs or database failures.
    pub async fn merge_club(&self, source_id: i64, target_id: i64) -> Result<()> {
        ensure!(
            source_id != target_id,
            "Quell- und Zielverein müssen unterschiedlich sein."
        );
        let mut tx = self.pool.begin().await?;
        let source_name = club_name(&mut tx, source_id).await?;
        club_name(&mut tx, target_id).await?;
        sqlx::query("UPDATE results SET club_id = ? WHERE club_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE teams SET club_id = ? WHERE club_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO club_aliases (club_id, alias, association_code, source, status)
             SELECT ?, alias, association_code, COALESCE(source, 'web-merge'), status
             FROM club_aliases WHERE club_id = ?",
        )
        .bind(target_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM club_aliases WHERE club_id = ? AND EXISTS (SELECT 1 FROM club_aliases target_alias WHERE target_alias.club_id = ? AND target_alias.alias = club_aliases.alias AND target_alias.id != club_aliases.id)")
            .bind(source_id).bind(target_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE club_aliases SET club_id = ? WHERE club_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO club_aliases (club_id, alias, association_code, source, status)
             SELECT id, ?, association_code, 'web-merge', 'active' FROM clubs WHERE id = ?",
        )
        .bind(&source_name)
        .bind(target_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO manual_overrides
             (scope, entity_type, entity_id, field_name, old_value, new_value, reason, status)
             SELECT 'entity', 'club', ?, 'merged_into', ?, canonical_name,
                    'Verein zusammengeführt', 'applied' FROM clubs WHERE id = ?",
        )
        .bind(source_id)
        .bind(&source_name)
        .bind(target_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM clubs WHERE id = ?")
            .bind(source_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn required_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(!name.is_empty(), "Bitte einen Namen eingeben.");
    Ok(name)
}

async fn club_name(connection: &mut SqliteConnection, club_id: i64) -> Result<String> {
    sqlx::query_scalar("SELECT canonical_name FROM clubs WHERE id = ?")
        .bind(club_id)
        .fetch_optional(connection)
        .await?
        .context("Verein nicht gefunden.")
}

async fn ensure_name_available(
    connection: &mut SqliteConnection,
    club_id: i64,
    name: &str,
    alias_id: Option<i64>,
) -> Result<()> {
    let other_club: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM clubs WHERE canonical_name = ? AND id != ?)
         OR EXISTS(SELECT 1 FROM club_aliases WHERE alias = ? AND status = 'active' AND club_id != ?)",
    ).bind(name).bind(club_id).bind(name).bind(club_id).fetch_one(&mut *connection).await?;
    ensure!(
        !other_club,
        "Dieser Name gehört bereits zu einem anderen Verein. Bitte dessen Eintrag bearbeiten; Vereine werden hier nicht zusammengeführt."
    );
    if let Some(id) = alias_id {
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM club_aliases WHERE alias = ? AND status = 'active' AND id != ?)",
        ).bind(name).bind(id).fetch_one(connection).await?;
        ensure!(
            !duplicate,
            "Dieser Alias ist für den Verein bereits vorhanden."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clubs (id, canonical_name) VALUES (1, 'Original'), (2, 'Other')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn rename_preserves_identity_raw_results_and_aliases() {
        let pool = database().await;
        sqlx::query("INSERT INTO competitions (id, code, name, year, scope) VALUES (1, 'LM', 'LM', 2026, 'LM')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO results (club_id, competition_id, result_kind, raw_club_name, raw_payload) VALUES (1, 1, 'individual', 'Original', 'raw')")
            .execute(&pool).await.unwrap();
        let repo = StorageRepository::new(&pool);
        repo.save_club_alias(1, None, "Variant").await.unwrap();
        repo.rename_club(1, " Renamed ").await.unwrap();
        repo.rename_club(1, "Final").await.unwrap();
        assert_eq!(repo.find_club_id_by_name("Final").await.unwrap(), Some(1));
        let aliases = repo.active_club_aliases().await.unwrap();
        assert_eq!(aliases.len(), 3);
        assert!(
            aliases
                .iter()
                .all(|alias| alias.club_id == 1 && alias.canonical_name == "Final")
        );
        let raw: (i64, String, String) =
            sqlx::query_as("SELECT club_id, raw_club_name, raw_payload FROM results")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(raw, (1, "Original".into(), "raw".into()));
        let audit: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM manual_overrides WHERE entity_id = 1 AND status = 'applied'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit, 2);
        pool.close().await;
    }

    #[tokio::test]
    async fn conflicting_and_invalid_edits_do_not_change_ownership() {
        let pool = database().await;
        let repo = StorageRepository::new(&pool);
        repo.save_club_alias(2, None, "Taken").await.unwrap();
        let id = repo.active_club_aliases().await.unwrap()[0].id;
        for name in ["Other", "Taken", "  "] {
            assert!(repo.rename_club(1, name).await.is_err());
            assert!(repo.save_club_alias(1, None, name).await.is_err());
        }
        assert!(repo.save_club_alias(1, Some(id), "Hijacked").await.is_err());
        assert!(repo.deactivate_owned_club_alias(1, id).await.is_err());
        assert!(repo.rename_club(999, "Missing").await.is_err());
        assert_eq!(
            repo.find_club_id_by_name("Original").await.unwrap(),
            Some(1)
        );
        let aliases = repo.active_club_aliases().await.unwrap();
        assert_eq!(aliases[0].alias, "Taken");
        assert_eq!(aliases[0].club_id, 2);
        pool.close().await;
    }

    #[tokio::test]
    async fn aliases_can_be_edited_deactivated_and_created_again() {
        let pool = database().await;
        let repo = StorageRepository::new(&pool);
        repo.save_club_alias(1, None, "Alias").await.unwrap();
        repo.save_club_alias(1, None, "Alias").await.unwrap();
        let aliases = repo.active_club_aliases().await.unwrap();
        assert_eq!(aliases.len(), 1);
        let id = aliases[0].id;
        repo.save_club_alias(1, Some(id), "Corrected")
            .await
            .unwrap();
        repo.save_club_alias(1, None, "Duplicate").await.unwrap();
        assert!(
            repo.save_club_alias(1, Some(id), "Duplicate")
                .await
                .is_err()
        );
        repo.deactivate_owned_club_alias(1, id).await.unwrap();
        assert!(
            repo.save_club_alias(1, Some(id), "Inactive edit")
                .await
                .is_err()
        );
        repo.save_club_alias(1, None, "Corrected").await.unwrap();
        assert_eq!(repo.all_club_aliases().await.unwrap().len(), 3);
        pool.close().await;
    }

    #[tokio::test]
    async fn merge_moves_results_teams_and_aliases_to_target() {
        let pool = database().await;
        sqlx::query("INSERT INTO competitions (id, code, name, year, scope) VALUES (1, 'LM', 'LM', 2026, 'LM')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO teams (id, canonical_name, club_id, competition_id) VALUES (1, 'Original I', 1, 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO results (club_id, competition_id, result_kind) VALUES (1, 1, 'individual')")
            .execute(&pool).await.unwrap();
        let repo = StorageRepository::new(&pool);
        repo.save_club_alias(1, None, "Schreibweise").await.unwrap();
        repo.merge_club(1, 2).await.unwrap();
        assert_eq!(repo.find_club_id_by_name("Original").await.unwrap(), None);
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM results WHERE club_id = 2), (SELECT COUNT(*) FROM teams WHERE club_id = 2), (SELECT COUNT(*) FROM club_aliases WHERE club_id = 2 AND status = 'active')")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (1, 1, 2));
        let aliases = repo.active_club_aliases().await.unwrap();
        assert!(aliases.iter().any(|alias| alias.alias == "Original"));
        assert!(aliases.iter().any(|alias| alias.alias == "Schreibweise"));
        let audit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manual_overrides WHERE entity_id = 1 AND field_name = 'merged_into'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(audit, 1);
        pool.close().await;
    }
}
