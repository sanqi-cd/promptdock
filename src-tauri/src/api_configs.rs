use rusqlite::{Connection, OptionalExtension};
use tauri::State;
use uuid::Uuid;

use crate::commands::DbState;
use crate::models::ApiConfig;

// ------------------ Validation ------------------

/// Upper bound for every free-text config field. Generous for names, providers
/// and model ids, and enough headroom for a real base URL.
const MAX_FIELD_LEN: usize = 200;

/// Validates user-supplied config fields before they reach the database.
///
/// The desktop UI validates too, but it is not a trust boundary: anything able
/// to call `invoke` reaches these commands directly.
///
/// Length is counted in characters, not bytes, so a 200-character Chinese name
/// is not rejected for being 600 bytes long.
fn validate_config_fields(
    name: &str,
    provider: &str,
    base_url: &str,
    model: &str,
) -> Result<(), String> {
    validate_required("配置名称", name)?;
    validate_required("Provider", provider)?;
    validate_required("模型名称", model)?;
    validate_required("Base URL", base_url)?;

    validate_base_url(base_url)
}

/// Validates a model endpoint.
///
/// The prefix check alone accepted `https://api.openai.com@evil.example` (the
/// real host is `evil.example`) and arbitrary URL syntax that reqwest would
/// later fail on. Userinfo is rejected so the host in the input is always the
/// host the key is sent to.
pub(crate) fn validate_base_url(base_url: &str) -> Result<(), String> {
    let parsed =
        reqwest::Url::parse(base_url).map_err(|_| "Base URL 不是合法的 URL".to_string())?;

    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Base URL 必须以 http:// 或 https:// 开头".to_string()),
    }

    if parsed.host_str().is_none() {
        return Err("Base URL 缺少主机名".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Base URL 不能包含用户名或密码".to_string());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Base URL 不能包含查询参数或片段".to_string());
    }

    Ok(())
}

fn validate_required(label: &str, value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label}不能为空"));
    }
    if trimmed.chars().count() > MAX_FIELD_LEN {
        return Err(format!("{label}不能超过 {MAX_FIELD_LEN} 个字符"));
    }
    Ok(())
}

/// Whether two URLs point at the same scheme/host/port. Path changes are
/// allowed (e.g. `/v1` → `/v1beta`) without re-entering the key.
fn same_origin(a: &str, b: &str) -> bool {
    match (reqwest::Url::parse(a), reqwest::Url::parse(b)) {
        (Ok(a), Ok(b)) => {
            a.scheme() == b.scheme()
                && a.host_str().map(str::to_ascii_lowercase)
                    == b.host_str().map(str::to_ascii_lowercase)
                && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

/// A blank key means "keep the existing key" when updating, but is invalid when
/// saving a new config.
fn validate_api_key(api_key: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    Ok(())
}

// ------------------ Commands ------------------

#[tauri::command]
pub fn list_api_configs(state: State<DbState>) -> Result<Vec<ApiConfig>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, provider, base_url, model, is_active, created_at
             FROM api_configs ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], row_to_api_config)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("读取 API 配置失败: {}", e))
}

fn row_to_api_config(row: &rusqlite::Row) -> rusqlite::Result<ApiConfig> {
    Ok(ApiConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        provider: row.get(2)?,
        base_url: row.get(3)?,
        model: row.get(4)?,
        is_active: row.get::<_, i32>(5)? != 0,
        created_at: row.get(6)?,
    })
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
    // Validate *before* touching the database. The previous version inserted
    // first and relied on the transaction being dropped (rolled back) to reject
    // a blank key — correct only by accident, and it would silently persist the
    // bad row the moment anyone moved the commit earlier.
    let name = name.trim().to_string();
    let provider = provider.trim().to_string();
    let base_url = base_url.trim().to_string();
    let model = model.trim().to_string();
    let api_key = api_key.trim().to_string();

    validate_config_fields(&name, &provider, &base_url, &model)?;
    validate_api_key(&api_key)?;

    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    let now = sql_now(&conn);

    // If this is the first config, set it as active.
    // 不能 unwrap_or(0)：查询失败会被当成「没有任何配置」，从而把这条配置
    // 静默设为 active，打乱用户已有的激活状态，且真实错误被吞掉。
    let existing_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM api_configs", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    let is_active = existing_count == 0;

    conn.execute(
        "INSERT INTO api_configs (id, name, provider, base_url, api_key, model, is_active, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, name, provider, base_url, api_key, model, is_active as i32, now],
    )
    .map_err(|e| e.to_string())?;

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
    let name = name.trim().to_string();
    let provider = provider.trim().to_string();
    let base_url = base_url.trim().to_string();
    let model = model.trim().to_string();
    let api_key = api_key.trim().to_string();

    validate_config_fields(&name, &provider, &base_url, &model)?;

    let mut conn = state.0.lock().map_err(|e| e.to_string())?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // 空 Key 表示沿用旧值。但 Key 与它被允许发往的主机绑定：如果 URL 的
    // origin 变了（比如 webview 内恶意脚本把地址改成攻击者主机），必须让
    // 用户重新输入 Key，避免旧 Key 被静默转发到新地址。
    if api_key.is_empty() {
        let (old_url, old_key): (String, String) = tx
            .query_row(
                "SELECT base_url, api_key FROM api_configs WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "API 配置不存在".to_string())?;

        if !old_key.trim().is_empty() && !same_origin(&old_url, &base_url) {
            return Err("修改 Base URL 的主机后请重新输入 API Key".to_string());
        }
    }

    let affected = tx.execute(
        "UPDATE api_configs SET name = ?1, provider = ?2, base_url = ?3, model = ?4 WHERE id = ?5",
        rusqlite::params![name, provider, base_url, model, id],
    )
    .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("API 配置不存在".to_string());
    }

    // 留空表示沿用已有 Key（api_key 上面已 trim）。
    if !api_key.is_empty() {
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
        .map_err(|e| match e {
            // 只有「确实没有激活的配置」才是用户可操作的提示；数据库被锁、
            // schema 不匹配等错误必须保留原因，否则排障会被误导。
            rusqlite::Error::QueryReturnedNoRows => {
                "请先在设置中配置并激活一个模型 API".to_string()
            }
            other => format!("读取 API 配置失败: {}", other),
        })?;

    if config.api_key.trim().is_empty() {
        return Err("未找到 API Key，请在设置中重新输入并保存该模型配置".to_string());
    }

    Ok(config)
}

fn sql_now(conn: &Connection) -> String {
    conn.query_row("SELECT datetime('now', 'localtime')", [], |row| row.get(0))
        .unwrap_or_else(|_| String::from("unknown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_NAME: &str = "我的 GPT";
    const VALID_PROVIDER: &str = "openai";
    const VALID_URL: &str = "https://api.openai.com/v1";
    const VALID_MODEL: &str = "gpt-4.1-mini";

    #[test]
    fn validate_config_fields_accepts_a_well_formed_config() {
        assert!(validate_config_fields(VALID_NAME, VALID_PROVIDER, VALID_URL, VALID_MODEL).is_ok());
    }

    #[test]
    fn validate_config_fields_rejects_blank_fields() {
        let cases = [
            ("", VALID_PROVIDER, VALID_URL, VALID_MODEL),
            ("   ", VALID_PROVIDER, VALID_URL, VALID_MODEL),
            (VALID_NAME, "", VALID_URL, VALID_MODEL),
            (VALID_NAME, "  ", VALID_URL, VALID_MODEL),
            (VALID_NAME, VALID_PROVIDER, "", VALID_MODEL),
            (VALID_NAME, VALID_PROVIDER, VALID_URL, ""),
            (VALID_NAME, VALID_PROVIDER, VALID_URL, "   "),
        ];

        for (name, provider, base_url, model) in cases {
            assert!(
                validate_config_fields(name, provider, base_url, model).is_err(),
                "空白字段应被拒绝: name={name:?} provider={provider:?} url={base_url:?} model={model:?}"
            );
        }
    }

    #[test]
    fn validate_config_fields_requires_an_http_scheme() {
        // reqwest 无法请求没有 scheme 的地址，与其等到调用模型时才报错，不如保存时就拒绝。
        let error = validate_config_fields(VALID_NAME, "ollama", "localhost:11434/v1", "llama3")
            .unwrap_err();

        assert!(
            error.contains("http"),
            "错误信息应说明需要 http(s) scheme: {error}"
        );
    }

    #[test]
    fn validate_base_url_rejects_userinfo_so_the_displayed_host_is_the_real_host() {
        // 前缀检查曾允许这种写法：真实主机是 evil.example，前缀却是
        // https://api.openai.com，用户会误以为 Key 发给了 OpenAI。
        let err = validate_base_url("https://api.openai.com@evil.example/v1").unwrap_err();
        assert!(err.contains("用户名"), "应拒绝 userinfo: {err}");
    }

    #[test]
    fn validate_base_url_rejects_query_and_fragment() {
        assert!(validate_base_url("https://api.openai.com/v1?x=1").is_err());
        assert!(validate_base_url("https://api.openai.com/v1#frag").is_err());
    }

    #[test]
    fn same_origin_ignores_path_but_not_scheme_host_or_port() {
        assert!(same_origin(
            "https://api.openai.com/v1",
            "https://api.openai.com/v1beta"
        ));
        // 默认端口等价：https://host 与 https://host:443 视为同源。
        assert!(same_origin(
            "https://api.openai.com",
            "https://api.openai.com:443"
        ));

        assert!(!same_origin(
            "https://api.openai.com/v1",
            "http://api.openai.com/v1"
        ));
        assert!(!same_origin(
            "https://api.openai.com/v1",
            "https://evil.example/v1"
        ));
        assert!(!same_origin(
            "https://api.openai.com/v1",
            "https://api.openai.com:8443/v1"
        ));
    }

    #[test]
    fn validate_base_url_allows_local_http_endpoints() {
        assert!(validate_base_url("http://localhost:11434/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:1234/v1").is_ok());
        assert!(validate_base_url("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url("ftp://example.com").is_err());
    }

    #[test]
    fn validate_config_fields_rejects_overlong_values() {
        let long = "a".repeat(MAX_FIELD_LEN + 1);

        assert!(validate_config_fields(&long, VALID_PROVIDER, VALID_URL, VALID_MODEL).is_err());
        assert!(validate_config_fields(VALID_NAME, &long, VALID_URL, VALID_MODEL).is_err());
        assert!(validate_config_fields(VALID_NAME, VALID_PROVIDER, &long, VALID_MODEL).is_err());
        assert!(validate_config_fields(VALID_NAME, VALID_PROVIDER, VALID_URL, &long).is_err());
    }

    #[test]
    fn validate_config_fields_counts_characters_not_bytes() {
        // 中文按字符计长，否则 50 个汉字的名称会因为 150 字节而被误判超长。
        let name = "名".repeat(MAX_FIELD_LEN);

        assert!(
            validate_config_fields(&name, VALID_PROVIDER, VALID_URL, VALID_MODEL).is_ok(),
            "{MAX_FIELD_LEN} 个汉字应当合法"
        );
    }

    #[test]
    fn validate_api_key_rejects_blank_values() {
        assert!(validate_api_key("").is_err());
        assert!(validate_api_key("   ").is_err());
        assert!(validate_api_key("sk-abc").is_ok());
    }
}
