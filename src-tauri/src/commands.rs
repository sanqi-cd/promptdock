use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;
use tauri::State;
use uuid::Uuid;

use crate::models::*;
use crate::search::search_prompts_impl;

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
    create_prompt_impl(&conn, title, content, optimized_content)
}

fn create_prompt_impl(
    conn: &Connection,
    title: String,
    content: String,
    optimized_content: Option<String>,
) -> Result<Prompt, String> {
    // 空记录在列表里既没有标题也没有内容，只能显示「未命名」，用户无法找回。
    // 前端已拦截，但 IPC 不是可信边界，迟到的保存队列也可能传空值。
    if title.trim().is_empty() && content.trim().is_empty() {
        return Err("Prompt 内容不能为空".to_string());
    }

    let id = Uuid::new_v4().to_string();
    let now = conn_now(conn)?;

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
    update_prompt_impl(&conn, &id, &title, &content, optimized_content)
}

fn update_prompt_impl(
    conn: &Connection,
    id: &str,
    title: &str,
    content: &str,
    optimized_content: Option<String>,
) -> Result<(), String> {
    let now = conn_now(conn)?;

    let affected = conn
        .execute(
            "UPDATE prompts SET title = ?1, content = ?2, optimized_content = ?3, updated_at = ?4 WHERE id = ?5",
            rusqlite::params![title, content, optimized_content, now, id],
        )
        .map_err(|e| e.to_string())?;

    // 记录已被删除 / 数据库被重置时必须报错。否则自动保存和 ⌘S 都会
    // 显示「已保存」，实际写入 0 行，用户的数据静默丢失。
    if affected == 0 {
        return Err("Prompt 不存在".to_string());
    }

    Ok(())
}

#[tauri::command]
pub fn delete_prompt(state: State<DbState>, id: String) -> Result<(), String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    delete_prompt_impl(&tx, &id)?;
    tx.commit().map_err(|e| e.to_string())
}

fn delete_prompt_impl(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM prompt_tags WHERE prompt_id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    let affected = conn
        .execute("DELETE FROM prompts WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("Prompt 不存在".to_string());
    }
    conn.execute(
        "DELETE FROM tags WHERE NOT EXISTS (
             SELECT 1 FROM prompt_tags pt WHERE pt.tag_id = tags.id
         )",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_prompt(state: State<DbState>, id: String) -> Result<Prompt, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    get_prompt_impl(&conn, &id)
}

fn get_prompt_impl(conn: &Connection, id: &str) -> Result<Prompt, String> {
    conn.query_row(
        "SELECT id, title, content, optimized_content, category, summary, is_favorite, created_at, updated_at, last_used_at FROM prompts WHERE id = ?1",
        rusqlite::params![id],
        row_to_prompt,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Prompt 不存在".to_string())
}

#[tauri::command]
pub fn list_prompts(
    state: State<DbState>,
    category: Option<String>,
    favorite_only: Option<bool>,
) -> Result<Vec<PromptSummary>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    list_prompts_impl(&conn, category, favorite_only)
}

fn list_prompts_impl(
    conn: &Connection,
    category: Option<String>,
    favorite_only: Option<bool>,
) -> Result<Vec<PromptSummary>, String> {
    let mut sql = String::from(
        "SELECT id, title, substr(content, 1, 200), category, summary, is_favorite, created_at, updated_at, last_used_at FROM prompts WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref cat) = category {
        sql.push_str(&format!(" AND category = ?{}", params.len() + 1));
        params.push(Box::new(cat.clone()));
    }
    if favorite_only.unwrap_or(false) {
        sql.push_str(" AND is_favorite = 1");
    }

    sql.push_str(" ORDER BY updated_at DESC, id");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            row_to_prompt_summary,
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("读取 Prompt 列表失败: {}", e))
}

#[tauri::command]
pub fn record_prompt_use(state: State<DbState>, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let now = conn_now(&conn)?;
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
    toggle_favorite_impl(&conn, &id)
}

fn toggle_favorite_impl(conn: &Connection, id: &str) -> Result<bool, String> {
    let affected = conn
        .execute(
            "UPDATE prompts SET is_favorite = CASE WHEN is_favorite = 1 THEN 0 ELSE 1 END WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;

    // 不存在的 id 不应该走到 query_row，否则用户看到的是
    // "Query returned no rows" 这种泄露实现的英文错误。
    if affected == 0 {
        return Err("Prompt 不存在".to_string());
    }

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
    get_prompt_tags_impl(&conn, &prompt_id)
}

fn get_prompt_tags_impl(conn: &Connection, prompt_id: &str) -> Result<Vec<Tag>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name FROM tags t
             INNER JOIN prompt_tags pt ON t.id = pt.tag_id
             WHERE pt.prompt_id = ?1
             ORDER BY t.name, t.id",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![prompt_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| format!("读取标签失败: {}", e))
}

/// 标签数量上限，与自动分类提示词里的「最多5个标签」保持一致。
const TAG_MAX_COUNT: usize = 5;
/// 单个标签的存储字符上限。模型提示仍建议不超过 15 字，后端给真实工具名
/// （如 Microsoft Copilot）和手工标签留足余量；必须按字符而非字节计算。
const TAG_MAX_CHARS: usize = 64;
const CATEGORY_MAX_CHARS: usize = 50;
const SUMMARY_MAX_CHARS: usize = 200;

#[tauri::command]
pub fn set_prompt_tags(
    state: State<DbState>,
    prompt_id: String,
    tag_names: Vec<String>,
) -> Result<Vec<Tag>, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let tags = replace_prompt_tags(&tx, &prompt_id, &tag_names)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(tags)
}

/// 用 `tag_names` 整体替换某个 Prompt 的标签，并返回最终生效的标签。
///
/// 调用方负责提供事务边界：清空 + 重建必须原子，否则中途失败会留下半个标签集。
fn replace_prompt_tags(
    conn: &Connection,
    prompt_id: &str,
    tag_names: &[String],
) -> Result<Vec<Tag>, String> {
    let prompt_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM prompts WHERE id = ?1)",
            rusqlite::params![prompt_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !prompt_exists {
        return Err("Prompt 不存在".to_string());
    }

    // 空白、超长和重复项都不能占用数量上限；否则返回值会包含 INSERT OR
    // IGNORE 实际未写入的重复标签，或提前丢弃后面的有效标签。
    let mut normalized: Vec<String> = Vec::new();
    for raw_name in tag_names {
        let name = raw_name.trim();
        if name.is_empty()
            || name.chars().count() > TAG_MAX_CHARS
            || normalized.iter().any(|existing| existing == name)
        {
            continue;
        }
        normalized.push(name.to_string());
        if normalized.len() == TAG_MAX_COUNT {
            break;
        }
    }

    conn.execute(
        "DELETE FROM prompt_tags WHERE prompt_id = ?1",
        rusqlite::params![prompt_id],
    )
    .map_err(|e| e.to_string())?;

    let mut tags = Vec::with_capacity(normalized.len());
    for name in &normalized {
        let tag_id = upsert_tag(conn, name)?;
        conn.execute(
            "INSERT INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![prompt_id, tag_id],
        )
        .map_err(|e| e.to_string())?;
        tags.push(Tag {
            id: tag_id,
            name: name.clone(),
        });
    }

    // 标签只能通过 Prompt 关联创建；不再被任何 Prompt 使用的标签没有用途，
    // 一并清理可避免替换标签后不断累积孤儿行。
    conn.execute(
        "DELETE FROM tags WHERE NOT EXISTS (
             SELECT 1 FROM prompt_tags pt WHERE pt.tag_id = tags.id
         )",
        [],
    )
    .map_err(|e| e.to_string())?;

    // normalized 已去重，以上 INSERT 任一失败都会回滚整个调用方事务，
    // 因此这里按输入优先级返回的标签与实际写入的关联完全一致。
    Ok(tags)
}

fn upsert_tag(conn: &Connection, name: &str) -> Result<String, String> {
    let new_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tags (id, name) VALUES (?1, ?2)
         ON CONFLICT(name) DO NOTHING",
        rusqlite::params![new_id, name],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        rusqlite::params![name],
        |row| row.get::<_, String>(0),
    )
    .map_err(|e| e.to_string())
}

// ------------------ Auto Classify ------------------

/// 一次性写入自动分类的全部结果：分类、摘要、标签（含推荐工具）。
///
/// 合并成单个命令是必须的 —— 以前前端先调 `update_prompt_meta`（把推荐工具写成
/// 标签），再调 `set_prompt_tags`（清空全部标签后写入 AI 标签），第二步会把第一步
/// 刚写进去的推荐工具标签删掉，并且两次写入之间失败会留下不一致的状态。
///
/// 刻意不碰 `optimized_content`：分类和润色是两件事，之前传 `None` 会把用户的
/// 润色结果静默抹成 NULL。
#[tauri::command]
pub fn apply_classification(
    state: State<DbState>,
    id: String,
    category: String,
    summary: String,
    recommended_tool: String,
    tag_names: Vec<String>,
) -> Result<Vec<Tag>, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let tags =
        apply_classification_impl(&tx, &id, &category, &summary, &recommended_tool, &tag_names)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(tags)
}

fn apply_classification_impl(
    conn: &Connection,
    id: &str,
    category: &str,
    summary: &str,
    recommended_tool: &str,
    tag_names: &[String],
) -> Result<Vec<Tag>, String> {
    // IPC 和模型输出都不是可信边界。分类/摘要必须非空，长度按 Unicode
    // 字符截断，不能用字符串字节下标切片（中文或 emoji 会 panic/截坏）。
    let category = normalize_classification_text("分类", category, CATEGORY_MAX_CHARS)?;
    let summary = normalize_classification_text("摘要", summary, SUMMARY_MAX_CHARS)?;

    let tool = recommended_tool.trim();
    if tool.chars().count() > TAG_MAX_CHARS {
        return Err(format!("推荐工具名称不能超过 {TAG_MAX_CHARS} 个字符"));
    }

    // 推荐工具排在最前，超出数量上限时优先保留它。名称要么完整保留、要么
    // 明确报错，绝不静默截成另一个看似有效但错误的工具名。
    let mut names: Vec<String> = Vec::with_capacity(tag_names.len() + 1);
    if !tool.is_empty() {
        names.push(tool.to_string());
    }
    for name in tag_names {
        let trimmed = name.trim();
        if !trimmed.is_empty() && !names.iter().any(|n| n == trimmed) {
            names.push(trimmed.to_string());
        }
    }

    let now = conn_now(conn)?;
    let affected = conn
        .execute(
            "UPDATE prompts SET category = ?1, summary = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![category, summary, now, id],
        )
        .map_err(|e| e.to_string())?;

    if affected == 0 {
        return Err("Prompt 不存在".to_string());
    }

    replace_prompt_tags(conn, id, &names)
}

fn normalize_classification_text(
    label: &str,
    value: &str,
    max_chars: usize,
) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label}不能为空"));
    }
    Ok(trimmed.chars().take(max_chars).collect())
}

// ------------------ Search ------------------

/// 搜索实现见 `search.rs`：分词与兜底规则的坑都记在那里。
#[tauri::command]
pub fn search_prompts(state: State<DbState>, query: String) -> Result<Vec<SearchResult>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    search_prompts_impl(&conn, &query)
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

fn row_to_prompt_summary(row: &rusqlite::Row) -> rusqlite::Result<PromptSummary> {
    Ok(PromptSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        content_preview: row.get(2)?,
        category: row.get(3)?,
        summary: row.get(4)?,
        is_favorite: row.get::<_, i32>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        last_used_at: row.get(8)?,
    })
}

fn conn_now(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("读取数据库时间失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    // 搜索已拆到 `search.rs`，相关用例仍留在本模块，验证的是那条「CJK 必须绕过
    // FTS」的路由没有被改回去。
    use crate::search::{build_fts_query, contains_cjk, escape_like};

    const FIXED_TIME: &str = "2026-01-01 00:00:00";

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        crate::db::init_schema(&conn).expect("init schema");
        conn
    }

    fn insert_prompt(conn: &Connection, id: &str, title: &str, content: &str) {
        conn.execute(
            "INSERT INTO prompts (id, title, content, created_at, updated_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?4, ?4)",
            rusqlite::params![id, title, content, FIXED_TIME],
        )
        .expect("insert prompt");
    }

    // ------------------ CJK detection ------------------

    #[test]
    fn contains_cjk_detects_chinese_and_mixed_queries() {
        assert!(contains_cjk("视频"));
        assert!(contains_cjk("AI 视频"));
        assert!(contains_cjk("帮我写一个关于"));
        assert!(contains_cjk("日本語"));
        assert!(contains_cjk("한국어"));
    }

    #[test]
    fn contains_cjk_detects_modern_ideograph_extensions_and_hangul_jamo() {
        for codepoint in [
            0x20000, // Extension B，超出 BMP，需两个 UTF-16 code unit
            0x2A700, // Extension C
            0x2EBF0, // Extension I
            0x30000, // Extension G
            0x31350, // Extension H
        ] {
            let character = char::from_u32(codepoint).expect("valid CJK codepoint");
            assert!(
                contains_cjk(&character.to_string()),
                "U+{codepoint:05X} 应走 LIKE 路由"
            );
        }
        for query in ["ᄀ", "ㄱ", "ꥠ", "ힰ"] {
            assert!(contains_cjk(query), "{query:?} 应识别为 Hangul Jamo");
        }
    }

    #[test]
    fn contains_cjk_leaves_ascii_queries_on_the_fts_path() {
        assert!(!contains_cjk("python"));
        assert!(!contains_cjk("gpt-4 prompt"));
        assert!(!contains_cjk(""));
    }

    // ------------------ LIKE escaping ------------------

    #[test]
    fn escape_like_neutralizes_wildcards() {
        assert_eq!(escape_like("30%"), "30\\%");
        assert_eq!(escape_like("snake_case"), "snake\\_case");
        assert_eq!(escape_like("100%_done"), "100\\%\\_done");
    }

    #[test]
    fn escape_like_escapes_backslash_before_wildcards() {
        // 反斜杠必须最先处理，否则后面引入的转义反斜杠会被二次转义。
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        assert_eq!(escape_like("\\%"), "\\\\\\%");
    }

    // ------------------ Search routing ------------------

    #[test]
    fn chinese_query_matches_substring_inside_an_unbroken_cjk_run() {
        // 回归测试：FTS5 的 unicode61 分词器把「帮我写一个关于」整段当作一个
        // token，前缀查询匹配不到中间的「视频」，必须路由到 LIKE 才能召回。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "视频脚本", "帮我写一个关于 AI 的视频脚本");
        insert_prompt(&conn, "p2", "翻译", "把这段英文翻译成中文");

        let results = search_prompts_impl(&conn, "视频").expect("search");

        assert_eq!(results.len(), 1, "应召回包含「视频」的那条");
        assert_eq!(results[0].id, "p1");
    }

    #[test]
    fn search_results_carry_favorite_flag() {
        // 搜索结果此前不返回 is_favorite，前端只能硬编码 false，
        // 同一条 Prompt 在「全部」里有星标、搜索出来星标消失。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "视频脚本", "帮我写一个关于 AI 的视频脚本");
        insert_prompt(&conn, "p2", "视频剪辑", "整理素材");
        conn.execute("UPDATE prompts SET is_favorite = 1 WHERE id = 'p2'", [])
            .expect("favorite p2");

        // CJK → LIKE 路径
        let results = search_prompts_impl(&conn, "视频").expect("like search");
        let favorite_of = |id: &str| results.iter().find(|r| r.id == id).unwrap().is_favorite;
        assert!(!favorite_of("p1"));
        assert!(favorite_of("p2"));

        // 纯 ASCII → FTS 路径
        let results = search_prompts_impl(&conn, "AI").expect("fts search");
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_favorite);

        // 空查询 → 最近使用路径
        let results = search_prompts_impl(&conn, "").expect("recent");
        let favorite_of = |id: &str| results.iter().find(|r| r.id == id).unwrap().is_favorite;
        assert!(favorite_of("p2"));
        assert!(!favorite_of("p1"));
    }

    #[test]
    fn multi_term_chinese_query_is_not_dropped_by_fts_and_semantics() {
        // 回归测试：FTS 下 `"AI"* "视频"*` 是隐式 AND，「视频」必然落空 → 零结果。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "视频脚本", "帮我写一个关于 AI 的视频脚本");
        insert_prompt(&conn, "p2", "翻译", "把这段英文翻译成中文");

        let results = search_prompts_impl(&conn, "AI 视频").expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "p1");
    }

    #[test]
    fn chinese_query_does_not_silently_drop_rows_fts_partially_matched() {
        // 这是 CJK 路由最关键的回归点：LIKE 兜底只在 FTS 返回「空」时执行。
        // 「视频」若恰好是某条记录的独立 token（前面有空格），FTS 会命中它，
        // 于是兜底被跳过，另一条包含「视频」的记录被静默丢弃 —— 用户看到的是
        // 一份不完整却没有提示的结果。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "视频教程", "视频 Python 入门教程");
        insert_prompt(&conn, "p2", "脚本", "帮我写一个关于 AI 的视频脚本");

        let results = search_prompts_impl(&conn, "视频").expect("search");

        assert_eq!(
            results.len(),
            2,
            "两条都含「视频」，不应只召回 FTS 命中的那条"
        );
    }

    #[test]
    fn multi_term_chinese_query_requires_every_term_to_appear() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "视频脚本", "帮我写一个关于 AI 的视频脚本");
        insert_prompt(&conn, "p2", "只有视频", "这里只提到视频，没有那个缩写");

        let results = search_prompts_impl(&conn, "AI 视频").expect("search");

        assert_eq!(results.len(), 1, "缺少任一词的记录不应被召回");
        assert_eq!(results[0].id, "p1");
    }

    #[test]
    fn ascii_query_still_uses_fts_prefix_matching() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "Python helper", "write a python script");
        insert_prompt(&conn, "p2", "Video script", "write a video script");

        let results = search_prompts_impl(&conn, "pyth").expect("search");

        assert_eq!(results.len(), 1, "ASCII 前缀查询仍由 FTS5 处理");
        assert_eq!(results[0].id, "p1");
    }

    #[test]
    fn wildcard_only_query_does_not_match_every_record() {
        // 回归测试：未转义时 LIKE 模式退化成 `%%%`，会返回整张表。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "折扣方案", "转化率提升 30% 的写法");
        insert_prompt(&conn, "p2", "无关记录", "这里没有任何百分号");

        let results = search_prompts_impl(&conn, "%").expect("search");

        assert_eq!(results.len(), 1, "「%」应作为字面量匹配，而不是通配符");
        assert_eq!(results[0].id, "p1");
    }

    #[test]
    fn search_attaches_tags_to_results() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "视频脚本", "帮我写一个关于 AI 的视频脚本");
        let tag_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
            rusqlite::params![tag_id, "短视频"],
        )
        .expect("insert tag");
        conn.execute(
            "INSERT INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params!["p1", tag_id],
        )
        .expect("link tag");

        let results = search_prompts_impl(&conn, "视频").expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tags, vec!["短视频".to_string()]);
    }

    #[test]
    fn ascii_search_does_not_let_partial_fts_hits_hide_tag_only_matches() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "Python guide", "write a python script");
        insert_prompt(&conn, "p2", "Tagged only", "unrelated body");
        apply_classification_impl(&conn, "p2", "分类", "摘要", "", &["python".to_string()])
            .expect("attach python tag");

        let results = search_prompts_impl(&conn, "python").expect("search");

        assert_eq!(results.len(), 2, "正文命中和标签命中必须同时返回");
        assert!(results.iter().any(|r| r.id == "p1"));
        assert!(results.iter().any(|r| r.id == "p2"));
    }

    #[test]
    fn mixed_query_applies_and_semantics_across_content_and_tags() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "Automation", "write a python script");
        insert_prompt(&conn, "p2", "No tag", "write a python script");
        apply_classification_impl(&conn, "p1", "分类", "摘要", "", &["scheduled".to_string()])
            .expect("attach scheduled tag");

        let results = search_prompts_impl(&conn, "python scheduled").expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "p1");
        assert_eq!(results[0].tags, vec!["scheduled".to_string()]);
    }

    #[test]
    fn cjk_tag_only_query_is_recalled_and_tags_are_stably_sorted() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "无关标题", "无关正文");
        apply_classification_impl(
            &conn,
            "p1",
            "分类",
            "摘要",
            "",
            &["短视频".to_string(), "自动化".to_string()],
        )
        .expect("attach tags");

        let results = search_prompts_impl(&conn, "短视频").expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "p1");
        assert_eq!(
            results[0].tags,
            vec!["短视频".to_string(), "自动化".to_string()]
        );
    }

    #[test]
    fn blank_query_returns_recent_prompts() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "第一条", "内容一");

        let results = search_prompts_impl(&conn, "   ").expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "p1");
    }

    #[test]
    fn list_prompts_returns_only_summary_fields_and_a_bounded_preview() {
        let conn = test_conn();
        let content = "界".repeat(250);
        insert_prompt(&conn, "p1", "长正文", &content);
        conn.execute(
            "UPDATE prompts SET optimized_content = '不应出现在列表中', category = '分类', summary = '摘要' WHERE id = 'p1'",
            [],
        )
        .expect("set metadata");

        let prompts = list_prompts_impl(&conn, None, None).expect("list prompts");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].content_preview, "界".repeat(200));

        let value = serde_json::to_value(&prompts[0]).expect("serialize summary");
        let object = value.as_object().expect("summary object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "category",
                "content_preview",
                "created_at",
                "id",
                "is_favorite",
                "last_used_at",
                "summary",
                "title",
                "updated_at",
            ]
        );
        assert!(!object.contains_key("content"));
        assert!(!object.contains_key("optimized_content"));
    }

    // ------------------ Error surfacing ------------------

    #[test]
    fn list_prompts_surfaces_rows_that_fail_to_map() {
        // 回归测试：`filter_map(|r| r.ok())` 会静默丢弃映射失败的行 —— 列表里
        // 少一条，但用户和日志都不会知道。SQLite 是动态类型，可以塞一行坏数据。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "正常", "内容");
        conn.execute(
            "INSERT INTO prompts (id, title, content, is_favorite, created_at, updated_at)
             VALUES ('bad', '坏行', '内容', 'not-an-integer', ?1, ?1)",
            rusqlite::params![FIXED_TIME],
        )
        .expect("insert malformed row");

        list_prompts_impl(&conn, None, None)
            .expect_err("映射失败的行必须让错误浮出，而不是被静默丢弃");
    }

    // ------------------ Tags ------------------

    #[test]
    fn apply_classification_preserves_the_full_recommended_tool_name() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        let tags = apply_classification_impl(&conn, "p1", "分类", "摘要", "Microsoft Copilot", &[])
            .expect("classify");

        assert_eq!(tags.len(), 1, "推荐工具不应被丢弃");
        assert_eq!(tags[0].name, "Microsoft Copilot");
    }

    #[test]
    fn apply_classification_rejects_an_overlong_tool_instead_of_truncating_it() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");
        let overlong = "工".repeat(TAG_MAX_CHARS + 1);

        let error = apply_classification_impl(&conn, "p1", "分类", "摘要", &overlong, &[])
            .expect_err("过长的工具名必须明确报错");
        assert!(error.contains("推荐工具名称"));
        let tags = get_prompt_tags_impl(&conn, "p1").expect("read tags");
        assert!(tags.is_empty(), "失败时不能写入截断的标签");
    }

    #[test]
    fn apply_classification_requires_nonblank_category_and_summary() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        for (category, summary, expected) in [
            ("  ", "有效摘要", "分类不能为空"),
            ("有效分类", "\n ", "摘要不能为空"),
        ] {
            let error = apply_classification_impl(&conn, "p1", category, summary, "", &[])
                .expect_err("空白元数据必须被拒绝");
            assert_eq!(error, expected);
        }
    }

    #[test]
    fn apply_classification_truncates_metadata_by_unicode_characters() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");
        let category = format!("  {}  ", "界".repeat(CATEGORY_MAX_CHARS + 20));
        let summary = "🙂".repeat(SUMMARY_MAX_CHARS + 20);

        apply_classification_impl(&conn, "p1", &category, &summary, "", &[])
            .expect("classify unicode metadata");

        let (stored_category, stored_summary): (String, String) = conn
            .query_row(
                "SELECT category, summary FROM prompts WHERE id = 'p1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read metadata");
        assert_eq!(stored_category, "界".repeat(CATEGORY_MAX_CHARS));
        assert_eq!(stored_summary, "🙂".repeat(SUMMARY_MAX_CHARS));
        assert_eq!(stored_summary.chars().count(), SUMMARY_MAX_CHARS);
    }

    #[test]
    fn apply_classification_keeps_the_recommended_tool_tag() {
        // 回归测试：以前 recommended_tool 先被写成标签，紧接着前端调用的
        // set_prompt_tags 会 DELETE 该 Prompt 的全部标签，于是这个标签从未真正
        // 生效过 —— 写进去、立刻被删掉。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        let returned = apply_classification_impl(
            &conn,
            "p1",
            "内容创作",
            "一句话摘要",
            "Claude",
            &["短视频".to_string()],
        )
        .expect("apply classification");

        let returned_names: Vec<&str> = returned.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(returned_names.first(), Some(&"Claude"));
        assert!(
            returned_names.contains(&"Claude"),
            "返回的标签里应含推荐工具: {returned_names:?}"
        );
        assert!(returned_names.contains(&"短视频"));

        let stored = get_prompt_tags_impl(&conn, "p1").expect("read tags");
        let stored_names: Vec<&str> = stored.iter().map(|t| t.name.as_str()).collect();
        assert!(
            stored_names.contains(&"Claude"),
            "标签必须真的落库，而不是先写后删: {stored_names:?}"
        );
    }

    #[test]
    fn apply_classification_does_not_wipe_optimized_content() {
        // 回归测试：以前 update_prompt_meta 传 None 会把 optimized_content 直接
        // 写成 NULL，任何只更新分类的调用都会静默清掉用户的润色结果。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");
        conn.execute(
            "UPDATE prompts SET optimized_content = '已润色' WHERE id = 'p1'",
            [],
        )
        .expect("seed optimized content");

        apply_classification_impl(&conn, "p1", "内容创作", "摘要", "", &[]).expect("classify");

        let optimized: Option<String> = conn
            .query_row(
                "SELECT optimized_content FROM prompts WHERE id = 'p1'",
                [],
                |row| row.get(0),
            )
            .expect("read optimized content");
        assert_eq!(optimized.as_deref(), Some("已润色"));
    }

    #[test]
    fn apply_classification_writes_category_and_summary() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        apply_classification_impl(&conn, "p1", "代码开发", "写一个脚本", "", &[])
            .expect("classify");

        let (category, summary): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT category, summary FROM prompts WHERE id = 'p1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read meta");
        assert_eq!(category.as_deref(), Some("代码开发"));
        assert_eq!(summary.as_deref(), Some("写一个脚本"));
    }

    #[test]
    fn tag_length_limit_counts_characters_not_bytes() {
        // 回归测试：达到字符上限的中文标签在 UTF-8 下远超字节上限，
        // 用 `name.len()` 判长会把合法标签静默丢弃。
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        let unicode_tag = "界".repeat(TAG_MAX_CHARS);
        assert_eq!(unicode_tag.chars().count(), TAG_MAX_CHARS);
        assert!(unicode_tag.len() > TAG_MAX_CHARS);

        let tags = apply_classification_impl(
            &conn,
            "p1",
            "内容创作",
            "摘要",
            "",
            std::slice::from_ref(&unicode_tag),
        )
        .expect("classify");

        assert_eq!(tags.len(), 1, "达到字符上限的中文标签应被接受");
        assert_eq!(tags[0].name, unicode_tag);
    }

    #[test]
    fn tag_limits_cap_count_and_drop_blank_or_overlong_names() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        let mut names: Vec<String> = (0..8).map(|i| format!("标签{i}")).collect();
        names.push("  ".to_string());
        names.push("标".repeat(TAG_MAX_CHARS + 1));

        let tags =
            apply_classification_impl(&conn, "p1", "分类", "摘要", "", &names).expect("classify");

        assert!(
            tags.len() <= TAG_MAX_COUNT,
            "标签数量应被限制: {}",
            tags.len()
        );
        assert!(
            tags.iter().all(|t| t.name.chars().count() <= TAG_MAX_CHARS),
            "超长标签应被丢弃而不是截断"
        );
        assert!(tags.iter().all(|t| !t.name.trim().is_empty()));
    }

    #[test]
    fn replace_prompt_tags_normalizes_before_limit_and_matches_stored_rows() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");
        let overlong = "长".repeat(TAG_MAX_CHARS + 1);
        let input = vec![
            "  ".to_string(),
            " alpha ".to_string(),
            "alpha".to_string(),
            overlong,
            "beta".to_string(),
            "gamma".to_string(),
            "delta".to_string(),
            "epsilon".to_string(),
            "zeta".to_string(),
        ];

        let returned = replace_prompt_tags(&conn, "p1", &input).expect("replace tags");
        let stored = get_prompt_tags_impl(&conn, "p1").expect("read stored tags");
        let mut returned_pairs: Vec<(&str, &str)> = returned
            .iter()
            .map(|tag| (tag.id.as_str(), tag.name.as_str()))
            .collect();
        let mut stored_pairs: Vec<(&str, &str)> = stored
            .iter()
            .map(|tag| (tag.id.as_str(), tag.name.as_str()))
            .collect();
        returned_pairs.sort_unstable();
        stored_pairs.sort_unstable();

        assert_eq!(returned_pairs, stored_pairs);
        assert_eq!(returned.len(), TAG_MAX_COUNT);
        assert!(!returned.iter().any(|tag| tag.name == "zeta"));
        assert!(returned.iter().any(|tag| tag.name == "epsilon"));
    }

    #[test]
    fn replace_prompt_tags_reuses_one_tag_per_name_and_cleans_orphans() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题一", "内容");
        insert_prompt(&conn, "p2", "标题二", "内容");

        let first = replace_prompt_tags(&conn, "p1", &["shared".to_string(), "old".to_string()])
            .expect("set first tags");
        let second =
            replace_prompt_tags(&conn, "p2", &["shared".to_string()]).expect("set second tags");
        assert_eq!(
            first.iter().find(|tag| tag.name == "shared").unwrap().id,
            second[0].id
        );

        replace_prompt_tags(&conn, "p1", &["shared".to_string()]).expect("replace first tags");
        let names: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE name = 'shared'",
                [],
                |row| row.get(0),
            )
            .expect("count shared tags");
        let orphans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags t WHERE NOT EXISTS (
                     SELECT 1 FROM prompt_tags pt WHERE pt.tag_id = t.id
                 )",
                [],
                |row| row.get(0),
            )
            .expect("count orphan tags");
        assert_eq!(names, 1);
        assert_eq!(orphans, 0);
    }

    #[test]
    fn replace_prompt_tags_rejects_a_missing_prompt_even_with_no_tags() {
        let conn = test_conn();

        let error = replace_prompt_tags(&conn, "missing", &[]).expect_err("missing prompt");
        assert_eq!(error, "Prompt 不存在");
    }

    // ------------------ Delete ------------------

    #[test]
    fn delete_removes_the_prompt_and_its_tag_links() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");
        apply_classification_impl(&conn, "p1", "分类", "摘要", "", &["标签".to_string()])
            .expect("classify");

        delete_prompt_impl(&conn, "p1").expect("delete");

        assert!(get_prompt_impl(&conn, "p1").is_err(), "Prompt 应已被删除");
        let links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompt_tags WHERE prompt_id = 'p1'",
                [],
                |row| row.get(0),
            )
            .expect("count links");
        assert_eq!(links, 0, "标签关联应被一并清理");
        let orphan_tags: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags t WHERE NOT EXISTS (
                     SELECT 1 FROM prompt_tags pt WHERE pt.tag_id = t.id
                 )",
                [],
                |row| row.get(0),
            )
            .expect("count orphan tags");
        assert_eq!(orphan_tags, 0, "删除最后一条引用后不应留下孤儿标签");
    }

    #[test]
    fn delete_rejects_a_missing_prompt() {
        let conn = test_conn();

        let error = delete_prompt_impl(&conn, "missing").expect_err("missing prompt");
        assert_eq!(error, "Prompt 不存在");
    }

    #[test]
    fn build_fts_query_quotes_terms_and_neutralizes_syntax() {
        assert_eq!(build_fts_query("hello world"), "\"hello\"* \"world\"*");
        // 引号被剥离，避免用户注入 FTS5 查询语法
        assert_eq!(build_fts_query("a\"b"), "\"ab\"*");
        assert_eq!(build_fts_query("   "), "   ");
    }

    // ------------------ Write-path validation ------------------

    #[test]
    fn create_prompt_rejects_completely_empty_input() {
        let conn = test_conn();

        let err = create_prompt_impl(&conn, String::new(), "  \n ".to_string(), None)
            .expect_err("完全空白的 Prompt 不应落库");
        assert_eq!(err, "Prompt 内容不能为空");

        // 只有标题或只有正文都是合法输入。
        assert!(create_prompt_impl(&conn, "只有标题".to_string(), String::new(), None).is_ok());
        assert!(create_prompt_impl(&conn, String::new(), "只有正文".to_string(), None).is_ok());
    }

    #[test]
    fn update_prompt_errors_when_the_row_no_longer_exists() {
        // 回归测试：旧实现不检查 affected，删除后迟到的自动保存会「成功」
        // 返回，用户看到已保存但 0 行被更新。
        let conn = test_conn();

        let err = update_prompt_impl(&conn, "missing", "标题", "内容", None)
            .expect_err("更新不存在的记录必须报错");
        assert_eq!(err, "Prompt 不存在");
    }

    #[test]
    fn toggle_favorite_errors_with_a_readable_message_for_unknown_id() {
        let conn = test_conn();

        let err = toggle_favorite_impl(&conn, "missing").expect_err("收藏不存在的记录必须报错");
        assert_eq!(
            err, "Prompt 不存在",
            "不应泄露 rusqlite 的 QueryReturnedNoRows"
        );
    }

    #[test]
    fn toggle_favorite_round_trips_the_flag() {
        let conn = test_conn();
        insert_prompt(&conn, "p1", "标题", "内容");

        assert!(toggle_favorite_impl(&conn, "p1").expect("favorite"));
        assert!(!toggle_favorite_impl(&conn, "p1").expect("unfavorite"));
    }
}
