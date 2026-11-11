use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use rusqlite::Connection;
use tauri::State;
use uuid::Uuid;

use crate::commands::DbState;

/// 在应用数据目录中创建一致性备份。
///
/// `VACUUM INTO` 会在备份期间持有 SQLite 读锁，生成的文件包含完整数据库，
/// 不需要暂停所有读取，也不会把 WAL 中尚未 checkpoint 的内容漏掉。
#[tauri::command]
pub fn backup_database(
    app_handle: tauri::AppHandle,
    state: State<DbState>,
) -> Result<String, String> {
    let db_path = crate::db::get_db_path(&app_handle)?;
    let app_dir = db_path
        .parent()
        .ok_or_else(|| "无法定位应用数据目录".to_string())?;
    let backup_dir = app_dir.join("backups");
    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("无法创建备份目录 {}: {}", backup_dir.display(), e))?;
    restrict_directory_permissions(&backup_dir)?;

    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let timestamp: String = conn
        .query_row("SELECT strftime('%Y%m%dT%H%M%SZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(|e| format!("无法生成备份时间: {}", e))?;
    let backup_path = backup_dir.join(format!(
        "promptdock-backup-{}-{}.db",
        timestamp,
        Uuid::new_v4().simple()
    ));

    backup_connection(&conn, &backup_path)?;
    Ok(backup_path.to_string_lossy().into_owned())
}

fn backup_connection(conn: &Connection, backup_path: &Path) -> Result<(), String> {
    // VACUUM INTO 接受一个已存在但为空的文件。先以 create_new + 0600 创建，
    // 避免 SQLite 按进程 umask 短暂生成其他用户可读的明文备份。
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(backup_path).map_err(|e| {
        if e.kind() == ErrorKind::AlreadyExists {
            format!("备份文件已存在，拒绝覆盖 {}: {}", backup_path.display(), e)
        } else {
            format!("无法创建备份文件 {}: {}", backup_path.display(), e)
        }
    })?;
    if let Err(error) = restrict_file_permissions(backup_path) {
        let _ = fs::remove_file(backup_path);
        return Err(error);
    }

    let destination = backup_path.to_string_lossy().into_owned();
    if let Err(error) = conn.execute("VACUUM INTO ?1", [destination.as_str()]) {
        let _ = fs::remove_file(backup_path);
        return Err(format!("数据库备份失败: {}", error));
    }

    if let Err(error) = restrict_file_permissions(backup_path) {
        let _ = fs::remove_file(backup_path);
        return Err(error);
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("无法限制备份目录权限 {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("无法限制备份文件权限 {}: {}", path.display(), e))
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn backup_contains_committed_data_and_cleans_up_sidecars() {
        let conn = Connection::open_in_memory().expect("open memory db");
        crate::db::init_schema(&conn).expect("init schema");
        conn.execute(
            "INSERT INTO prompts (id, title, content) VALUES ('p1', '备份测试', '完整内容')",
            [],
        )
        .expect("insert prompt");

        let path =
            std::env::temp_dir().join(format!("promptdock-backup-test-{}.db", Uuid::new_v4()));
        backup_connection(&conn, &path).expect("backup database");
        drop(conn);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path)
                    .expect("read backup permissions")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        let backup = Connection::open(&path).expect("open backup");
        let title: String = backup
            .query_row("SELECT title FROM prompts WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .expect("read backup");
        assert_eq!(title, "备份测试");
        drop(backup);

        fs::remove_file(&path).expect("remove backup");
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        let shm = PathBuf::from(format!("{}-shm", path.display()));
        let _ = fs::remove_file(wal);
        let _ = fs::remove_file(shm);
    }
}
