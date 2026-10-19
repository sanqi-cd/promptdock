mod ai;
mod api_configs;
mod commands;
mod db;
mod models;
mod polish_prompts;
mod search;

use commands::DbState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 不要用 expect 直接 panic：数据库被锁/权限错误时给出一条可读的
            // 启动错误，比带 backtrace 的崩溃更容易排查。
            let db_path = db::get_db_path(app.handle()).map_err(std::io::Error::other)?;
            let conn = db::init_db(&db_path).map_err(|e| {
                std::io::Error::other(format!("无法初始化数据库 {}: {}", db_path.display(), e))
            })?;
            app.manage(DbState(Mutex::new(conn)));
            app.manage(ai::PolishCancelState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_prompt,
            commands::update_prompt,
            commands::delete_prompt,
            commands::get_prompt,
            commands::list_prompts,
            commands::record_prompt_use,
            commands::toggle_favorite,
            commands::get_prompt_tags,
            commands::set_prompt_tags,
            commands::apply_classification,
            commands::search_prompts,
            ai::analyze_polish_process,
            ai::polish_prompt,
            ai::polish_prompt_stream,
            ai::cancel_polish_stream,
            ai::auto_classify,
            ai::test_connection,
            api_configs::list_api_configs,
            api_configs::get_active_config,
            api_configs::save_api_config,
            api_configs::update_api_config,
            api_configs::delete_api_config,
            api_configs::activate_api_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
