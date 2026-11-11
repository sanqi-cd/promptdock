use rusqlite::{Connection, Result as SqliteResult};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const DB_FILENAME: &str = "promptdock.db";

/// 当前 schema 版本，存于 `PRAGMA user_version`。
///
/// 1 = 已确保 FTS 索引与 `prompts` 表一致。
/// 2 = 标签/激活配置唯一约束、查询索引、精确 FTS 更新触发器及 UTC 时间默认值。
const SCHEMA_VERSION: i64 = 2;

pub fn get_db_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    // 失败时不能回退到当前目录：那可能把含明文 Key 的数据库写进下载目录、
    // 同步盘或仓库目录。宁可直接报错退出。
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {}", e))?;

    fs::create_dir_all(&app_dir)
        .map_err(|e| format!("无法创建应用数据目录 {}: {}", app_dir.display(), e))?;

    // 数据库里有明文 API Key，目录只允许当前用户访问。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&app_dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("无法设置应用数据目录权限: {}", e))?;
    }

    Ok(app_dir.join(DB_FILENAME))
}

pub fn init_db(db_path: &Path) -> Result<Connection, String> {
    restrict_db_permissions(db_path)?;
    let conn = Connection::open(db_path)
        .map_err(|e| format!("无法打开数据库 {}: {}", db_path.display(), e))?;
    // A brand-new database file was created by the open above with umask
    // permissions; tighten it before the first write creates the WAL sidecar
    // (SQLite gives the sidecars the database file's mode).
    restrict_db_permissions(db_path)?;
    // Without a busy timeout, any concurrent writer (a second app instance or an
    // external sqlite tool) makes the next write fail immediately with
    // "database is locked". Wait briefly instead.
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("无法设置数据库忙等待: {}", e))?;
    init_schema(&conn).map_err(|e| format!("无法初始化数据库 schema: {}", e))?;
    // 覆盖本次启动新建的 WAL/SHM。chmod 失败必须终止初始化，不能让明文
    // API Key 留在权限过宽的文件里继续运行。
    restrict_db_permissions(db_path)?;
    Ok(conn)
}

/// Keeps the database and its WAL sidecars readable only by the current user.
///
/// SQLite creates `-wal` / `-shm` with the database file's permissions, so the
/// chmod has to run before opening the connection (and cover sidecars created
/// by a previous run).
fn restrict_db_permissions(db_path: &Path) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = db_path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::Permissions::from_mode(0o600);
        for path in [
            db_path.to_path_buf(),
            PathBuf::from(format!("{}-wal", db_path.display())),
            PathBuf::from(format!("{}-shm", db_path.display())),
        ] {
            match fs::set_permissions(&path, mode.clone()) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "无法设置数据库文件权限 {}: {}",
                        path.display(),
                        error
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Creates tables, indexes and FTS5 triggers on an already-open connection.
///
/// Split out of [`init_db`] so tests can exercise the real schema against an
/// in-memory database instead of re-declaring the DDL.
pub fn init_schema(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    // DDL、数据回填、FTS rebuild 与 user_version 必须在同一事务中提交。
    // 任何一步失败都回滚，避免留下“版本已升级但索引/数据未迁移”的半状态。
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let migration = migrate_schema(conn);
    match migration {
        Ok(()) => conn.execute_batch("COMMIT;"),
        Err(error) => {
            // 回滚失败同样不能忽略；此时连接已不可安全继续使用。
            conn.execute_batch("ROLLBACK;")?;
            Err(error)
        }
    }
}

fn migrate_schema(conn: &Connection) -> SqliteResult<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= SCHEMA_VERSION {
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS prompts (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            optimized_content TEXT,
            category TEXT,
            summary TEXT,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
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
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        );",
    )?;

    ensure_column(conn, "prompts", "last_used_at", "TEXT")?;
    ensure_column(conn, "api_configs", "api_key", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute(
        "UPDATE prompts SET last_used_at = updated_at WHERE last_used_at IS NULL",
        [],
    )?;

    // 老库可能在 foreign_keys=OFF 时留下失效关联。先清孤儿，再把重复名称
    // 的关联合并到每个名称确定性的 keeper（最小 id），最后才能建立唯一索引。
    conn.execute_batch(
        "DELETE FROM prompt_tags
         WHERE NOT EXISTS (SELECT 1 FROM prompts p WHERE p.id = prompt_tags.prompt_id)
            OR NOT EXISTS (SELECT 1 FROM tags t WHERE t.id = prompt_tags.tag_id);

         INSERT OR IGNORE INTO prompt_tags (prompt_id, tag_id)
         SELECT pt.prompt_id, keeper.id
         FROM prompt_tags pt
         INNER JOIN tags duplicate ON duplicate.id = pt.tag_id
         INNER JOIN tags keeper ON keeper.name = duplicate.name
         WHERE duplicate.id <> keeper.id
           AND keeper.id = (
               SELECT MIN(candidate.id) FROM tags candidate
               WHERE candidate.name = duplicate.name
           );

         DELETE FROM tags
         WHERE id NOT IN (SELECT MIN(id) FROM tags GROUP BY name);

         DELETE FROM tags
         WHERE NOT EXISTS (
             SELECT 1 FROM prompt_tags pt WHERE pt.tag_id = tags.id
         );

         CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_name_unique ON tags(name);
         CREATE INDEX IF NOT EXISTS idx_prompt_tags_tag_id ON prompt_tags(tag_id);

         -- 保留最近创建、id 字典序最大的一条 active，迁移和运行时读取规则一致。
         UPDATE api_configs SET is_active = 0
         WHERE is_active = 1
           AND id NOT IN (
               SELECT id FROM api_configs WHERE is_active = 1
               ORDER BY created_at DESC, id DESC LIMIT 1
           );

         CREATE UNIQUE INDEX IF NOT EXISTS idx_api_configs_one_active
         ON api_configs(is_active) WHERE is_active = 1;
         CREATE INDEX IF NOT EXISTS idx_api_configs_created_at
         ON api_configs(created_at DESC, id DESC);

         CREATE INDEX IF NOT EXISTS idx_prompts_updated_at
         ON prompts(updated_at DESC, id);
         CREATE INDEX IF NOT EXISTS idx_prompts_category_updated_at
         ON prompts(category, updated_at DESC, id);
         CREATE INDEX IF NOT EXISTS idx_prompts_favorite_updated_at
         ON prompts(is_favorite, updated_at DESC, id);

         CREATE VIRTUAL TABLE IF NOT EXISTS prompts_fts USING fts5(
             title,
             content,
             category,
             summary,
             content='prompts',
             content_rowid='rowid'
         );

         DROP TRIGGER IF EXISTS prompts_ai;
         DROP TRIGGER IF EXISTS prompts_ad;
         DROP TRIGGER IF EXISTS prompts_au;

         CREATE TRIGGER prompts_ai AFTER INSERT ON prompts BEGIN
             INSERT INTO prompts_fts(rowid, title, content, category, summary)
             VALUES (new.rowid, new.title, new.content, new.category, new.summary);
         END;

         CREATE TRIGGER prompts_ad AFTER DELETE ON prompts BEGIN
             INSERT INTO prompts_fts(prompts_fts, rowid, title, content, category, summary)
             VALUES ('delete', old.rowid, old.title, old.content, old.category, old.summary);
         END;

         CREATE TRIGGER prompts_au
         AFTER UPDATE OF title, content, category, summary ON prompts BEGIN
             INSERT INTO prompts_fts(prompts_fts, rowid, title, content, category, summary)
             VALUES ('delete', old.rowid, old.title, old.content, old.category, old.summary);
             INSERT INTO prompts_fts(rowid, title, content, category, summary)
             VALUES (new.rowid, new.title, new.content, new.category, new.summary);
         END;

         -- 外部内容 FTS 不会自动回填既有行；老库和新建空库统一重建一次。
         INSERT INTO prompts_fts(prompts_fts) VALUES('rebuild');",
    )?;

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

    struct TestDbPath(PathBuf);

    impl TestDbPath {
        fn new(test_name: &str) -> Self {
            let filename = format!(
                "promptdock-{test_name}-{}-{}.db",
                std::process::id(),
                uuid::Uuid::new_v4()
            );
            Self(std::env::temp_dir().join(filename))
        }
    }

    impl Drop for TestDbPath {
        fn drop(&mut self) {
            for path in [
                self.0.clone(),
                PathBuf::from(format!("{}-wal", self.0.display())),
                PathBuf::from(format!("{}-shm", self.0.display())),
            ] {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("清理测试数据库失败: {error}"),
                }
            }
        }
    }

    #[test]
    fn test_init_db() {
        let db_path = TestDbPath::new("init");
        let conn = init_db(&db_path.0).expect("Failed to init test db");

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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&db_path.0)
                .expect("read database permissions")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        // 先关闭 SQLite，再由 TestDbPath 清理主文件及 WAL/SHM。
        drop(conn);
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
    fn migration_deduplicates_tags_and_active_configs_and_removes_orphans() {
        let conn = Connection::open_in_memory().expect("open legacy database");
        conn.execute_batch(
            "PRAGMA foreign_keys=OFF;
            CREATE TABLE prompts (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                optimized_content TEXT,
                category TEXT,
                summary TEXT,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE tags (id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE prompt_tags (
                prompt_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                PRIMARY KEY (prompt_id, tag_id),
                FOREIGN KEY (prompt_id) REFERENCES prompts(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            );
            CREATE TABLE api_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL,
                model TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO prompts (id, title, content) VALUES
                ('p1', 'one', 'python'),
                ('p2', 'two', 'rust');
            INSERT INTO tags (id, name) VALUES
                ('tag-b', 'shared'),
                ('tag-a', 'shared'),
                ('orphan-tag', 'unused');
            INSERT INTO prompt_tags (prompt_id, tag_id) VALUES
                ('p1', 'tag-b'),
                ('p1', 'tag-a'),
                ('p2', 'tag-a'),
                ('missing-prompt', 'tag-a'),
                ('p1', 'missing-tag');
            INSERT INTO api_configs (
                id, name, provider, base_url, model, is_active, created_at
            ) VALUES
                ('old', 'old', 'openai', 'https://old.example', 'm', 1, '2025-01-01'),
                ('new', 'new', 'openai', 'https://new.example', 'm', 1, '2026-01-01');",
        )
        .expect("create legacy data");

        init_schema(&conn).expect("migrate legacy data");

        let shared: Vec<(String, String)> = conn
            .prepare(
                "SELECT pt.prompt_id, t.id FROM prompt_tags pt
                 INNER JOIN tags t ON t.id = pt.tag_id
                 WHERE t.name = 'shared' ORDER BY pt.prompt_id",
            )
            .expect("prepare shared tags")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query shared tags")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect shared tags");
        assert_eq!(
            shared,
            vec![
                ("p1".to_string(), "tag-a".to_string()),
                ("p2".to_string(), "tag-a".to_string())
            ]
        );

        let orphan_rows: i64 = conn
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM prompt_tags pt
                      WHERE NOT EXISTS (SELECT 1 FROM prompts p WHERE p.id = pt.prompt_id)
                         OR NOT EXISTS (SELECT 1 FROM tags t WHERE t.id = pt.tag_id))
                     +
                     (SELECT COUNT(*) FROM tags t
                      WHERE NOT EXISTS (SELECT 1 FROM prompt_tags pt WHERE pt.tag_id = t.id))",
                [],
                |row| row.get(0),
            )
            .expect("count orphan rows");
        assert_eq!(orphan_rows, 0);

        let active: Vec<String> = conn
            .prepare("SELECT id FROM api_configs WHERE is_active = 1")
            .expect("prepare active")
            .query_map([], |row| row.get(0))
            .expect("query active")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect active");
        assert_eq!(active, vec!["new".to_string()]);

        assert!(conn
            .execute("INSERT INTO tags (id, name) VALUES ('tag-c', 'shared')", [])
            .is_err());
        assert!(conn
            .execute("UPDATE api_configs SET is_active = 1 WHERE id = 'old'", [])
            .is_err());
    }

    #[test]
    fn fts_update_trigger_only_covers_indexed_columns() {
        let conn = Connection::open_in_memory().expect("open database");
        init_schema(&conn).expect("init schema");

        let trigger_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'prompts_au'",
                [],
                |row| row.get(0),
            )
            .expect("read update trigger");
        assert!(
            trigger_sql.contains("AFTER UPDATE OF title, content, category, summary ON prompts")
        );

        for index in [
            "idx_tags_name_unique",
            "idx_prompt_tags_tag_id",
            "idx_api_configs_one_active",
            "idx_prompts_updated_at",
            "idx_prompts_category_updated_at",
            "idx_prompts_favorite_updated_at",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1)",
                    [index],
                    |row| row.get(0),
                )
                .expect("check index");
            assert!(exists, "缺少索引 {index}");
        }
    }

    #[test]
    fn version_one_with_existing_fts_and_missing_triggers_is_fully_upgraded() {
        let conn = legacy_db_without_fts();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE prompts_fts USING fts5(
                title,
                content,
                category,
                summary,
                content='prompts',
                content_rowid='rowid'
            );
            PRAGMA user_version = 1;",
        )
        .expect("create version-one FTS without triggers");

        init_schema(&conn).expect("upgrade version one");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version, SCHEMA_VERSION);
        let trigger_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'trigger' AND name IN ('prompts_ai', 'prompts_ad', 'prompts_au')",
                [],
                |row| row.get(0),
            )
            .expect("count FTS triggers");
        assert_eq!(trigger_count, 3, "升级必须补齐三类 FTS 触发器");

        let legacy_hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts_fts WHERE prompts_fts MATCH '\"python\"*'",
                [],
                |row| row.get(0),
            )
            .expect("query rebuilt legacy FTS");
        assert_eq!(legacy_hits, 1, "已有空 FTS 也必须回填历史行");

        conn.execute(
            "INSERT INTO prompts (id, title, content) VALUES ('new', 'new', 'golang')",
            [],
        )
        .expect("insert after upgrade");
        let new_hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompts_fts WHERE prompts_fts MATCH '\"golang\"*'",
                [],
                |row| row.get(0),
            )
            .expect("query new FTS row");
        assert_eq!(new_hits, 1, "补齐的 INSERT 触发器必须立即可用");
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
