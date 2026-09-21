//! 搜索：FTS5 + LIKE 双路径。
//!
//! 匹配规则的坑集中在两处，改动前先看 [`search_prompts_impl`] 的注释：
//!
//! 1. FTS5 的 unicode61 分词器把一整段连续中文当作**单个 token**，前缀查询只能
//!    命中该段开头，所以含 CJK 的查询直接走 LIKE。
//! 2. LIKE 兜底**只在 FTS 返回空时触发**。FTS 若返回了部分结果，兜底不会执行，
//!    用户会拿到一份不完整却无提示的结果 —— 这才是 CJK 必须绕过 FTS 的根本原因，
//!    别把这条路由删掉。

use rusqlite::Connection;

use crate::models::SearchResult;

const SEARCH_RESULT_LIMIT: usize = 50;

pub(crate) fn search_prompts_impl(
    conn: &Connection,
    query: &str,
) -> Result<Vec<SearchResult>, String> {
    let mut results = if query.trim().is_empty() {
        recent_prompts(conn)?
    } else if contains_cjk(query) {
        // FTS5's default unicode61 tokenizer treats an unbroken run of CJK as a
        // single token, so the prefix queries built by `build_fts_query` only
        // match at the *start* of such a run: searching「视频」misses
        // 「...的视频脚本」, and multi-term queries like「AI 视频」become an
        // impossible AND. LIKE is the only correct matcher for CJK here, and it
        // is fast enough for a local prompt library.
        like_search(conn, query)?
    } else {
        let fts_results = fts_search(conn, query)?;
        if fts_results.is_empty() {
            like_search(conn, query)?
        } else {
            fts_results
        }
    };

    attach_tags(conn, &mut results)?;
    Ok(results)
}

/// Recent prompts, used when the search box is empty.
fn recent_prompts(conn: &Connection) -> Result<Vec<SearchResult>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.title, p.summary, p.category, p.updated_at
             FROM prompts p ORDER BY COALESCE(p.last_used_at, p.updated_at) DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    collect_search_results(stmt.query_map([SEARCH_RESULT_LIMIT], row_to_search_result))
}

fn fts_search(conn: &Connection, query: &str) -> Result<Vec<SearchResult>, String> {
    let fts_query = build_fts_query(query);
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.title, p.summary, p.category, p.updated_at
             FROM prompts_fts fts
             INNER JOIN prompts p ON p.rowid = fts.rowid
             WHERE prompts_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;

    collect_search_results(stmt.query_map(
        rusqlite::params![fts_query, SEARCH_RESULT_LIMIT],
        row_to_search_result,
    ))
}

/// Matches each whitespace-separated term as a literal substring; every term
/// must appear somewhere in the row (AND), mirroring the FTS path.
///
/// Callers pass raw user input, so LIKE metacharacters are escaped — otherwise a
/// query of `%` would return every row.
fn like_search(conn: &Connection, query: &str) -> Result<Vec<SearchResult>, String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| format!("%{}%", escape_like(term)))
        .collect();

    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut clauses: Vec<String> = Vec::new();
    for term in terms {
        let index = params.len() + 1;
        clauses.push(format!(
            "(title LIKE ?{index} ESCAPE '\\'
              OR content LIKE ?{index} ESCAPE '\\'
              OR summary LIKE ?{index} ESCAPE '\\'
              OR category LIKE ?{index} ESCAPE '\\')"
        ));
        params.push(Box::new(term));
    }
    params.push(Box::new(SEARCH_RESULT_LIMIT as i64));

    let sql = format!(
        "SELECT id, title, summary, category, updated_at FROM prompts
         WHERE {}
         ORDER BY updated_at DESC LIMIT ?{}",
        clauses.join(" AND "),
        params.len()
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    collect_search_results(stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        row_to_search_result,
    ))
}

/// Attaches tags to every result via a single batch query.
fn attach_tags(conn: &Connection, results: &mut [SearchResult]) -> Result<(), String> {
    if results.is_empty() {
        return Ok(());
    }

    let placeholders: Vec<String> = (0..results.len()).map(|i| format!("?{}", i + 1)).collect();
    let tag_sql = format!(
        "SELECT pt.prompt_id, t.name FROM tags t
         INNER JOIN prompt_tags pt ON t.id = pt.tag_id
         WHERE pt.prompt_id IN ({})",
        placeholders.join(",")
    );
    let mut tag_stmt = conn.prepare(&tag_sql).map_err(|e| e.to_string())?;
    let tag_rows: Vec<(String, String)> = tag_stmt
        .query_map(
            rusqlite::params_from_iter(results.iter().map(|r| &r.id as &dyn rusqlite::types::ToSql)),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;

    let mut tag_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (prompt_id, tag_name) in tag_rows {
        tag_map.entry(prompt_id).or_default().push(tag_name);
    }

    for result in results.iter_mut() {
        if let Some(tags) = tag_map.get(&result.id) {
            result.tags = tags.clone();
        }
    }

    Ok(())
}

fn row_to_search_result(row: &rusqlite::Row) -> rusqlite::Result<SearchResult> {
    Ok(SearchResult {
        id: row.get(0)?,
        title: row.get(1)?,
        summary: row.get(2)?,
        category: row.get(3)?,
        tags: Vec::new(),
        updated_at: row.get(4)?,
    })
}

/// Collects query rows, surfacing row-mapping failures instead of dropping them.
fn collect_search_results<F>(
    rows: rusqlite::Result<rusqlite::MappedRows<'_, F>>,
) -> Result<Vec<SearchResult>, String>
where
    F: FnMut(&rusqlite::Row) -> rusqlite::Result<SearchResult>,
{
    rows.map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub(crate) fn build_fts_query(query: &str) -> String {
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

/// True when `query` contains CJK characters, which the FTS5 default tokenizer
/// cannot match mid-token (see [`search_prompts_impl`]).
pub(crate) fn contains_cjk(query: &str) -> bool {
    query.chars().any(is_cjk)
}

fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x2E80..=0x2EFF     // CJK Radicals Supplement
            | 0x3000..=0x303F // CJK Symbols and Punctuation
            | 0x3040..=0x30FF // Hiragana and Katakana
            | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xAC00..=0xD7AF // Hangul Syllables
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
            | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
    )
}

/// Escapes LIKE metacharacters so user input matches literally.
///
/// Backslash must be escaped *first*, otherwise the backslashes introduced for
/// `%` and `_` would themselves be doubled.
pub(crate) fn escape_like(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
