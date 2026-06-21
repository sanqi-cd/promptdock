use rusqlite::Connection;
use std::sync::Mutex;
use tauri::State;
use uuid::Uuid;

use crate::models::*;

pub struct DbState(pub Mutex<Connection>);

// ------------------ Prompt CRUD ------------------

#[tauri::command]
pub fn create_prompt(
    state: State<DbState>,
    title: String,
    content: String,
    optimized_content: Option<String>,
) -> Result<Prompt, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let now = conn_now(&conn);

    conn.execute(
        "INSERT INTO prompts (id, title, content, optimized_content, created_at, updated_at, last_used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, title, content, optimized_content, now, now, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(Prompt {
        id,
        title,
        content,
        optimized_content,
        category: None,
        summary: None,
        is_favorite: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        last_used_at: Some(now),
    })
}

#[tauri::command]
pub fn update_prompt(
    state: State<DbState>,
    id: String,
    title: String,
    content: String,
    optimized_content: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let now = conn_now(&conn);

    conn.execute(
        "UPDATE prompts SET title = ?1, content = ?2, optimized_content = ?3, updated_at = ?4 WHERE id = ?5",
        rusqlite::params![title, content, optimized_content, now, id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_prompt(state: State<DbState>, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("BEGIN", []).map_err(|e| e.to_string())?;
    let result = (|| {
        conn.execute(
            "DELETE FROM prompt_tags WHERE prompt_id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM prompts WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| e.to_string())
    })();
    if let Err(e) = &result {
        conn.execute("ROLLBACK", []).ok();
        Err(e.clone())
    } else {
        conn.execute("COMMIT", []).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tauri::command]
pub fn get_prompt(state: State<DbState>, id: String) -> Result<Prompt, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, title, content, optimized_content, category, summary, is_favorite, created_at, updated_at, last_used_at FROM prompts WHERE id = ?1",
        rusqlite::params![id],
        row_to_prompt,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_prompts(
    state: State<DbState>,
    category: Option<String>,
    favorite_only: Option<bool>,
) -> Result<Vec<Prompt>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    let mut sql = String::from(
        "SELECT id, title, content, optimized_content, category, summary, is_favorite, created_at, updated_at, last_used_at FROM prompts WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref cat) = category {
        sql.push_str(&format!(" AND category = ?{}", params.len() + 1));
        params.push(Box::new(cat.clone()));
    }
    if favorite_only.unwrap_or(false) {
        sql.push_str(" AND is_favorite = 1");
    }

    sql.push_str(" ORDER BY updated_at DESC");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let prompts = stmt
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            row_to_prompt,
        )
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(prompts)
}

#[tauri::command]
pub fn record_prompt_use(state: State<DbState>, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let now = conn_now(&conn);
    let affected = conn
        .execute(
            "UPDATE prompts SET last_used_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )
        .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("Prompt 不存在".to_string());
    }

    Ok(())
}

#[tauri::command]
pub fn toggle_favorite(state: State<DbState>, id: String) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE prompts SET is_favorite = CASE WHEN is_favorite = 1 THEN 0 ELSE 1 END WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;

    let fav: i32 = conn
        .query_row(
            "SELECT is_favorite FROM prompts WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(fav != 0)
}

// ------------------ Tags ------------------

#[tauri::command]
pub fn get_prompt_tags(state: State<DbState>, prompt_id: String) -> Result<Vec<Tag>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name FROM tags t
             INNER JOIN prompt_tags pt ON t.id = pt.tag_id
             WHERE pt.prompt_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let tags = stmt
        .query_map(rusqlite::params![prompt_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tags)
}

#[tauri::command]
pub fn set_prompt_tags(
    state: State<DbState>,
    prompt_id: String,
    tag_names: Vec<String>,
) -> Result<Vec<Tag>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    // Remove existing tags
    conn.execute(
        "DELETE FROM prompt_tags WHERE prompt_id = ?1",
        rusqlite::params![prompt_id],
    )
    .map_err(|e| e.to_string())?;

    let mut tags = Vec::new();
    for name in tag_names.iter().take(5) {
        let name = name.trim();
        if name.is_empty() || name.len() > 15 {
            continue;
        }

        // Upsert tag
        let tag_id: String = match conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        ) {
            Ok(existing_id) => existing_id,
            Err(_) => {
                let new_id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO tags (id, name) VALUES (?1, ?2)",
                    rusqlite::params![new_id, name],
                )
                .map_err(|e| e.to_string())?;
                new_id
            }
        };

        // Link tag to prompt
        conn.execute(
            "INSERT OR IGNORE INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![prompt_id, tag_id],
        )
        .map_err(|e| e.to_string())?;

        tags.push(Tag {
            id: tag_id,
            name: name.to_string(),
        });
    }

    Ok(tags)
}

// ------------------ Auto Classify ------------------

#[tauri::command]
pub fn update_prompt_meta(
    state: State<DbState>,
    id: String,
    category: Option<String>,
    summary: Option<String>,
    optimized_content: Option<String>,
    recommended_tool: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let now = conn_now(&conn);

    conn.execute(
        "UPDATE prompts SET category = ?1, summary = ?2, optimized_content = ?3, updated_at = ?4 WHERE id = ?5",
        rusqlite::params![category, summary, optimized_content, now, id],
    )
    .map_err(|e| e.to_string())?;

    // Store recommended_tool as a tag if provided
    if let Some(tool) = recommended_tool {
        if !tool.is_empty() {
            let tool_id: String = match conn.query_row(
                "SELECT id FROM tags WHERE name = ?1",
                rusqlite::params![tool],
                |row| row.get(0),
            ) {
                Ok(existing_id) => existing_id,
                Err(_) => {
                    let new_id = Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO tags (id, name) VALUES (?1, ?2)",
                        rusqlite::params![new_id, tool],
                    )
                    .map_err(|e| e.to_string())?;
                    new_id
                }
            };

            conn.execute(
                "INSERT OR IGNORE INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
                rusqlite::params![id, tool_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ------------------ Search ------------------

#[tauri::command]
pub fn search_prompts(state: State<DbState>, query: String) -> Result<Vec<SearchResult>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    if query.trim().is_empty() {
        // Return recent prompts if query is empty
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.title, p.summary, p.category, p.updated_at
                 FROM prompts p ORDER BY COALESCE(p.last_used_at, p.updated_at) DESC LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let results: Vec<SearchResult> = stmt
            .query_map([], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: row.get(2)?,
                    category: row.get(3)?,
                    tags: vec![],
                    updated_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    } else {
        // FTS5 search
        let fts_query = build_fts_query(&query);
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.title, p.summary, p.category, p.updated_at
                 FROM prompts_fts fts
                 INNER JOIN prompts p ON p.rowid = fts.rowid
                 WHERE prompts_fts MATCH ?1
                 ORDER BY rank LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let mut results: Vec<SearchResult> = stmt
            .query_map(rusqlite::params![fts_query], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: row.get(2)?,
                    category: row.get(3)?,
                    tags: vec![],
                    updated_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        // Fallback: LIKE search if FTS returns nothing
        if results.is_empty() {
            results = like_search(&conn, &query)?;
        }

        // Attach tags to results via a single batch query
        if !results.is_empty() {
            let placeholders: Vec<String> =
                (0..results.len()).map(|i| format!("?{}", i + 1)).collect();
            let tag_sql = format!(
                "SELECT pt.prompt_id, t.name FROM tags t
                 INNER JOIN prompt_tags pt ON t.id = pt.tag_id
                 WHERE pt.prompt_id IN ({})",
                placeholders.join(",")
            );
            let mut tag_stmt = conn.prepare(&tag_sql).map_err(|e| e.to_string())?;
            let tag_rows: Vec<(String, String)> = tag_stmt
                .query_map(
                    rusqlite::params_from_iter(
                        results.iter().map(|r| &r.id as &dyn rusqlite::types::ToSql),
                    ),
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            let mut tag_map: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            for (pid, tag_name) in tag_rows {
                tag_map.entry(pid).or_default().push(tag_name);
            }
            for result in &mut results {
                if let Some(t) = tag_map.get(&result.id) {
                    result.tags = t.clone();
                }
            }
        }

        Ok(results)
    }
}

fn build_fts_query(query: &str) -> String {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return query.to_string();
    }
    // Quote each term to prevent FTS5 query syntax injection, append * for prefix matching
    terms
        .iter()
        .map(|t| format!("\"{}\"*", t.replace('\"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn like_search(conn: &Connection, query: &str) -> Result<Vec<SearchResult>, String> {
    let like_pattern = format!("%{}%", query);
    let mut stmt = conn
        .prepare(
            "SELECT id, title, summary, category, updated_at FROM prompts
             WHERE title LIKE ?1 OR content LIKE ?1 OR summary LIKE ?1 OR category LIKE ?1
             ORDER BY updated_at DESC LIMIT 50",
        )
        .map_err(|e| e.to_string())?;

    let results = stmt
        .query_map(rusqlite::params![like_pattern], |row| {
            Ok(SearchResult {
                id: row.get(0)?,
                title: row.get(1)?,
                summary: row.get(2)?,
                category: row.get(3)?,
                tags: vec![],
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}
// ------------------ Helpers ------------------

fn row_to_prompt(row: &rusqlite::Row) -> rusqlite::Result<Prompt> {
    Ok(Prompt {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        optimized_content: row.get(3)?,
        category: row.get(4)?,
        summary: row.get(5)?,
        is_favorite: row.get::<_, i32>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        last_used_at: row.get(9)?,
    })
}

fn conn_now(conn: &Connection) -> String {
    conn.query_row("SELECT datetime('now', 'localtime')", [], |row| row.get(0))
        .unwrap_or_else(|_| String::from("unknown"))
}
