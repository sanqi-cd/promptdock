use rusqlite::{Connection, Result as SqliteResult};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const DB_FILENAME: &str = "promptdock.db";

/// 当前 schema 版本，存于 `PRAGMA user_version`。
///
/// 1 = 已确保 FTS 索引与 `prompts` 表一致。
const SCHEMA_VERSION: i64 = 1;

pub fn get_db_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    if let Err(e) = fs::create_dir_all(&app_dir) {
        eprintln!("Failed to create app data directory: {}", e);
    }
    app_dir.join(DB_FILENAME)
}

pub fn init_db(db_path: &Path) -> SqliteResult<Connection> {
    let conn = Connection::open(db_path)?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Creates tables, indexes and FTS5 triggers on an already-open connection.
///
/// Split out of [`init_db`] so tests can exercise the real schema against an
/// in-memory database instead of re-declaring the DDL.
pub fn init_schema(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS prompts (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            optimized_content TEXT,
            category TEXT,
            summary TEXT,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            last_used_at TEXT
        );

        CREATE TABLE IF NOT EXISTS tags (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS prompt_tags (
            prompt_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            PRIMARY KEY (prompt_id, tag_id),
            FOREIGN KEY (prompt_id) REFERENCES prompts(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS api_configs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );",
    )?;

    ensure_column(conn,"prompts", "last_used_at", "TEXT")?;
    ensure_column(conn,"api_configs", "api_key", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute(
        "UPDATE prompts SET last_used_at = updated_at WHERE last_used_at IS NULL",
        [],
    )?;

    // FTS5 virtual table for full-text search
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS prompts_fts USING fts5(
            title,
            content,
            category,
            summary,
            content='prompts',
            content_rowid='rowid'
        );

        -- Triggers to keep FTS index in sync
        CREATE TRIGGER IF NOT EXISTS prompts_ai AFTER INSERT ON prompts BEGIN
            INSERT INTO prompts_fts(rowid, title, content, category, summary)
            VALUES (new.rowid, new.title, new.content, new.category, new.summary);
        END;

        CREATE TRIGGER IF NOT EXISTS prompts_ad AFTER DELETE ON prompts BEGIN
            INSERT INTO prompts_fts(prompts_fts, rowid, title, content, category, summary)
            VALUES ('delete', old.rowid, old.title, old.content, old.category, old.summary);
        END;

        CREATE TRIGGER IF NOT EXISTS prompts_au AFTER UPDATE ON prompts BEGIN
            INSERT INTO prompts_fts(prompts_fts, rowid, title, content, category, summary)
            VALUES ('delete', old.rowid, old.title, old.content, old.category, old.summary);
            INSERT INTO prompts_fts(rowid, title, content, category, summary)
            VALUES (new.rowid, new.title, new.content, new.category, new.summary);
        END;",
    )?;

    run_migrations(conn)?;

    Ok(())
}

/// 一次性数据迁移，由 `PRAGMA user_version` 驱动。
fn run_migrations(conn: &Connection) -> SqliteResult<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= SCHEMA_VERSION {
        return Ok(());
    }

    // FTS 索引对**既有行**是空的：外部内容表的索引不会自动回填，上面那些
    // `CREATE TRIGGER IF NOT EXISTS` 也只对之后的写入生效。老库（prompts 表先于
    // FTS 存在）因此会静默搜索不到任何历史数据，必须重建一次。
    conn.execute_batch("INSERT INTO prompts_fts(prompts_fts) VALUES('rebuild');")?;

    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;

    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> SqliteResult<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == column);

    if !exists {
        conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
            [],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_init_db() {
        let db_path = PathBuf::from("test_promptdock.db");
        let conn = init_db(&db_path).expect("Failed to init test db");

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"prompts".to_string()));
        assert!(tables.contains(&"tags".to_string()));
        assert!(tables.contains(&"prompt_tags".to_string()));
        assert!(tables.contains(&"prompts_fts".to_string()));

        let api_config_columns: Vec<String> = conn
            .prepare("PRAGMA table_info(api_configs)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(api_config_columns.contains(&"api_key".to_string()));

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
    }

    /// 模拟「prompts 表先于 FTS 存在」的老库：先只建 prompts 并写入数据，
    /// 再跑 init_schema。
    fn legacy_db_without_fts() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE prompts (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                optimized_content TEXT,
                category TEXT,
                summary TEXT,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
            );
            INSERT INTO prompts (id, title, content)
            VALUES ('legacy', '旧 Prompt', 'write a python script');",
        )
        .expect("create legacy schema");
        conn
    }

    #[test]
    fn init_schema_rebuilds_a_missing_fts_index() {
        // 回归测试：`CREATE VIRTUAL TABLE IF NOT EXISTS` 对老库只会建出一张空索引，
        // 且触发器只对**之后**的写入生效 —— 老数据全部搜不到，而且没有任何提示。
        let conn = legacy_db_without_fts();

        init_schema(&conn).expect("init schema");

        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts_fts WHERE prompts_fts MATCH '\"python\"*'",
                [],
                |row| row.get(0),
            )
            .expect("query fts");
        assert_eq!(hits, 1, "升级后老数据必须能被 FTS 搜到");
    }

    #[test]
    fn init_schema_records_the_schema_version_so_rebuild_runs_once() {
        let conn = legacy_db_without_fts();

        init_schema(&conn).expect("first init");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SCHEMA_VERSION);

        // 再跑一次不应报错（幂等），也不应重复清空索引。
        init_schema(&conn).expect("second init");
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts_fts WHERE prompts_fts MATCH '\"python\"*'",
                [],
                |row| row.get(0),
            )
            .expect("query fts");
        assert_eq!(hits, 1);
    }

    #[test]
    fn init_schema_keeps_new_writes_indexed() {
        let conn = legacy_db_without_fts();
        init_schema(&conn).expect("init schema");

        conn.execute(
            "INSERT INTO prompts (id, title, content) VALUES ('new', '新 Prompt', 'a golang service')",
            [],
        )
        .expect("insert");

        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts_fts WHERE prompts_fts MATCH '\"golang\"*'",
                [],
                |row| row.get(0),
            )
            .expect("query fts");
        assert_eq!(hits, 1, "触发器应继续同步新写入");
    }
}
