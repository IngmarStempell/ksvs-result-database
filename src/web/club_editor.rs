use std::fmt::Write as _;

use super::{
    ApplicationService, BTreeMap, Context, HttpRequest, Result, StorageRepository, WebResponse,
    escape_html, form_urlencoded, form_value, parse_form_urlencoded, render_template,
};

const EDITOR: &str = include_str!("../../templates/web-club-editor.html");
const ALIAS_EDITOR: &str = include_str!("../../templates/web-club-alias-editor.html");

pub(super) fn url(path: &str, query: &BTreeMap<String, String>) -> String {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query)
        .finish();
    format!("{path}?{encoded}")
}

#[allow(clippy::too_many_lines)]
pub(super) async fn render(
    pool: &sqlx::SqlitePool,
    id: i64,
    query: &BTreeMap<String, String>,
    detail: bool,
) -> Result<String> {
    let service = ApplicationService::new(pool);
    let Some(club) = service.club(id).await? else {
        return Ok("<p role=\"alert\">Verein nicht gefunden.</p>".to_owned());
    };
    let mut context = query.clone();
    for key in [
        "error",
        "draft",
        "draft_alias_id",
        "draft_operation",
        "saved",
        "edit",
    ] {
        context.remove(key);
    }
    if detail {
        context.insert("detail".into(), "1".into());
    }
    let action = escape_html(&url(&format!("/clubs/{}/edit", club.id), &context));
    let close_url = if detail {
        format!("/clubs/{id}")
    } else {
        url("/clubs", &context)
    };
    let notice = query.get("error").map_or_else(
        || {
            if query.contains_key("saved") {
                "<p role=\"status\">Änderung gespeichert.</p>".to_owned()
            } else {
                String::new()
            }
        },
        |error| format!("<p role=\"alert\">{}</p>", escape_html(error)),
    );
    let mut aliases_html = String::new();
    for alias in service.club_aliases(id).await? {
        if alias.status == "active" {
            let value = if query.get("draft_alias_id") == Some(&alias.id.to_string()) {
                query.get("draft").unwrap_or(&alias.alias)
            } else {
                &alias.alias
            };
            aliases_html.push_str(&render_template(
                ALIAS_EDITOR,
                &[
                    ("action", action.clone()),
                    ("alias_id", alias.id.to_string()),
                    ("alias", escape_html(value)),
                ],
            ));
        } else {
            let _ = write!(
                aliases_html,
                "<p class=\"muted\">{} (inaktiv)</p>",
                escape_html(&alias.alias)
            );
        }
    }
    if aliases_html.is_empty() {
        aliases_html.push_str("<p class=\"muted\">Noch keine Aliase vorhanden.</p>");
    }
    let mut merge_options = String::new();
    for option in service.club_options(id).await? {
        let _ = writeln!(
            merge_options,
            "<option value=\"{}\">{}</option>",
            option.id,
            escape_html(&option.canonical_name)
        );
    }
    let rename_draft = query
        .get("draft_operation")
        .is_some_and(|op| op == "rename");
    let new_alias_draft = query
        .get("draft_operation")
        .is_some_and(|op| op == "alias-save")
        && !query.contains_key("draft_alias_id");
    Ok(render_template(
        EDITOR,
        &[
            ("club_name", escape_html(&club.canonical_name)),
            (
                "name_value",
                escape_html(if rename_draft {
                    query.get("draft").unwrap_or(&club.canonical_name)
                } else {
                    &club.canonical_name
                }),
            ),
            (
                "alias_value",
                if new_alias_draft {
                    escape_html(query.get("draft").map_or("", String::as_str))
                } else {
                    String::new()
                },
            ),
            ("action", action),
            ("close_url", escape_html(&close_url)),
            ("notice", notice),
            ("aliases", aliases_html),
            ("merge_options", merge_options),
        ],
    ))
}

pub(super) async fn post(request: &HttpRequest, pool: &sqlx::SqlitePool) -> Result<WebResponse> {
    let id = request
        .path
        .strip_prefix("/clubs/")
        .and_then(|path| path.strip_suffix("/edit"))
        .context("invalid club edit path")?
        .parse::<i64>()?;
    let form = parse_form_urlencoded(&request.body);
    let mut query = request.query.clone();
    let detail = query.remove("detail").is_some_and(|value| value == "1");
    query.insert("edit".into(), id.to_string());
    match save(pool, id, &form).await {
        Ok(()) => {
            query.insert("saved".into(), "1".into());
            let path = if detail {
                format!("/clubs/{id}")
            } else {
                "/clubs".into()
            };
            Ok(WebResponse::Redirect(format!(
                "{}#club-editor",
                url(&path, &query)
            )))
        }
        Err(error) => {
            query.insert("error".into(), error.to_string());
            for (field, target) in [
                ("name", "draft"),
                ("alias_id", "draft_alias_id"),
                ("operation", "draft_operation"),
            ] {
                if let Some(value) = form.get(field) {
                    query.insert(target.into(), value.clone());
                }
            }
            let html = if detail {
                super::club_detail_page(pool, id, &query).await?
            } else {
                super::clubs_page(pool, &query).await?
            };
            Ok(WebResponse::Html(html))
        }
    }
}

async fn save(pool: &sqlx::SqlitePool, id: i64, form: &BTreeMap<String, String>) -> Result<()> {
    let repository = StorageRepository::new(pool);
    let alias_id = form
        .get("alias_id")
        .map(|value| value.parse::<i64>())
        .transpose()?;
    match form_value(form, "operation")?.as_str() {
        "rename" => repository.rename_club(id, &form_value(form, "name")?).await,
        "alias-save" => {
            repository
                .save_club_alias(id, alias_id, &form_value(form, "name")?)
                .await
        }
        "alias-deactivate" => {
            repository
                .deactivate_owned_club_alias(id, alias_id.context("Alias fehlt.")?)
                .await
        }
        "merge" => {
            repository
                .merge_club(id, form_value(form, "target_id")?.parse::<i64>()?)
                .await
        }
        _ => anyhow::bail!("Unbekannte Aktion."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn editor_preserves_context_escapes_names_and_displays_errors_in_place() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO clubs (id, canonical_name) VALUES (1, '<Club>'), (2, 'Taken')")
            .execute(&pool)
            .await
            .unwrap();
        let query = BTreeMap::from([
            ("q".into(), "<Club>".into()),
            ("page".into(), "2".into()),
            ("edit".into(), "1".into()),
        ]);
        let html = super::super::clubs_page(&pool, &query).await.unwrap();
        assert!(html.contains("&lt;Club&gt; bearbeiten"));
        assert!(!html.contains("<Club>"));
        assert!(html.contains("/clubs/1/edit?page=2&amp;q=%3CClub%3E"));
        assert!(!html.contains("{{"));
        let mut request = HttpRequest {
            method: "POST".into(),
            path: "/clubs/1/edit".into(),
            query,
            body: "operation=rename&name=Renamed".into(),
        };
        let WebResponse::Redirect(location) = post(&request, &pool).await.unwrap() else {
            panic!("expected redirect")
        };
        assert!(location.starts_with("/clubs?"));
        assert!(location.contains("page=2"));
        assert!(location.contains("q=%3CClub%3E"));
        assert!(location.ends_with("#club-editor"));
        request.body = "operation=rename&name=Taken".into();
        let WebResponse::Html(html) = post(&request, &pool).await.unwrap() else {
            panic!("expected inline error")
        };
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains("bereits zu einem anderen Verein"));
        assert!(html.contains("value=\"Taken\""));
        request.query = BTreeMap::from([("detail".into(), "1".into())]);
        request.body = "operation=alias-save&name=Variant".into();
        let WebResponse::Redirect(location) = post(&request, &pool).await.unwrap() else {
            panic!("expected detail redirect")
        };
        assert!(location.starts_with("/clubs/1?"));
        let html = super::super::club_detail_page(
            &pool,
            1,
            &BTreeMap::from([("edit".into(), "1".into())]),
        )
        .await
        .unwrap();
        assert!(html.contains("Renamed"));
        assert!(html.contains("Variant"));
        assert!(!html.contains("{{"));
        pool.close().await;
    }
}
