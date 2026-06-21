use rusqlite::{Connection, OptionalExtension};
use tauri::State;
use uuid::Uuid;

use crate::commands::DbState;
use crate::models::ApiConfig;

#[tauri::command]
pub fn list_api_configs(state: State<DbState>) -> Result<Vec<ApiConfig>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, provider, base_url, model, is_active, created_at
             FROM api_configs ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let configs = stmt
        .query_map([], |row| {
            Ok(ApiConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider: row.get(2)?,
                base_url: row.get(3)?,
                model: row.get(4)?,
                is_active: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(configs)
}

#[tauri::command]
pub fn get_active_config(state: State<DbState>) -> Result<Option<ApiConfig>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, name, provider, base_url, model, is_active, created_at
         FROM api_configs WHERE is_active = 1 LIMIT 1",
        [],
        |row| {
            Ok(ApiConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider: row.get(2)?,
                base_url: row.get(3)?,
                model: row.get(4)?,
                is_active: true,
                created_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_api_config(
    state: State<DbState>,
    name: String,
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
) -> Result<ApiConfig, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let now = sql_now(&conn);

    // If this is the first config, set it as active
    let existing_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM api_configs", [], |row| row.get(0))
        .unwrap_or(0);
    let is_active = existing_count == 0;

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO api_configs (id, name, provider, base_url, api_key, model, is_active, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, name, provider, base_url, api_key, model, is_active as i32, now],
    )
    .map_err(|e| e.to_string())?;

    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(ApiConfig {
        id,
        name,
        provider,
        base_url,
        model,
        is_active,
        created_at: now,
    })
}

#[tauri::command]
pub fn update_api_config(
    state: State<DbState>,
    id: String,
    name: String,
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let affected = tx.execute(
        "UPDATE api_configs SET name = ?1, provider = ?2, base_url = ?3, model = ?4 WHERE id = ?5",
        rusqlite::params![name, provider, base_url, model, id],
    )
    .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("API 配置不存在".to_string());
    }

    if !api_key.trim().is_empty() {
        tx.execute(
            "UPDATE api_configs SET api_key = ?1 WHERE id = ?2",
            rusqlite::params![api_key, id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_api_config(state: State<DbState>, id: String) -> Result<(), String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let was_active: bool = tx
        .query_row(
            "SELECT is_active FROM api_configs WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok(row.get::<_, i32>(0)? != 0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "API 配置不存在".to_string())?;

    tx.execute(
        "DELETE FROM api_configs WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;

    if was_active {
        tx.execute(
            "UPDATE api_configs SET is_active = 1
             WHERE id = (
                 SELECT id FROM api_configs ORDER BY created_at DESC LIMIT 1
             )",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.commit().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn activate_api_config(state: State<DbState>, id: String) -> Result<(), String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE api_configs SET is_active = 0", [])
        .map_err(|e| e.to_string())?;
    let affected = tx
        .execute(
            "UPDATE api_configs SET is_active = 1 WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("API 配置不存在".to_string());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

// Helper: get the active config's ApiSettings for use by ai.rs
pub fn get_active_settings(state: &State<DbState>) -> Result<crate::ai::ApiSettings, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let config = conn
        .query_row(
            "SELECT id, provider, base_url, api_key, model FROM api_configs WHERE is_active = 1 LIMIT 1",
            [],
            |row| {
                Ok(crate::ai::ApiSettings {
                    config_id: row.get(0)?,
                    provider: row.get(1)?,
                    base_url: row.get(2)?,
                    api_key: row.get(3)?,
                    model: row.get(4)?,
                })
            },
        )
        .map_err(|_| "请先在设置中配置并激活一个模型 API".to_string())?;

    if config.api_key.trim().is_empty() {
        return Err("未找到 API Key，请在设置中重新输入并保存该模型配置".to_string());
    }

    Ok(config)
}

fn sql_now(conn: &Connection) -> String {
    conn.query_row("SELECT datetime('now', 'localtime')", [], |row| row.get(0))
        .unwrap_or_else(|_| String::from("unknown"))
}
