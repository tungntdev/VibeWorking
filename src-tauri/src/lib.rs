//! AI Office 3D — văn phòng 3D với các nhân viên AI.

pub mod auth;
pub mod commands;
pub mod state;
pub mod db;
pub mod engine;
pub mod export;
pub mod memory;
pub mod optimizer;
pub mod osauth;
pub mod providers;
pub mod setup;
pub mod templates;
pub mod workspace;

use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(data_dir.join("workspaces"))?;
            let conn = db::open(&data_dir.join("office.db"))?;
            let path_env = providers::resolve_path_env();
            let handle = app.handle().clone();
            let core = state::Core::new(
                conn,
                data_dir,
                path_env,
                Box::new(move |v| {
                    let _ = handle.emit("office-event", v);
                }),
            );
            engine::seed(&core)?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::setup_manager,
            commands::login,
            commands::logout,
            commands::me,
            commands::change_password,
            commands::list_users,
            commands::save_user,
            commands::delete_user,
            commands::get_settings,
            commands::save_settings,
            commands::list_providers,
            commands::save_provider,
            commands::delete_provider,
            commands::test_provider,
            commands::detect_clis,
            commands::refresh_path,
            commands::list_agents,
            commands::list_templates,
            commands::save_agent,
            commands::delete_agent,
            commands::recruit,
            commands::hire,
            commands::list_missions,
            commands::get_mission,
            commands::create_mission,
            commands::update_mission,
            commands::delete_mission,
            commands::plan_mission,
            commands::run_mission,
            commands::cancel_mission,
            commands::save_subtask,
            commands::delete_subtask,
            commands::ask_agent,
            commands::list_activity,
            commands::dashboard,
            commands::scan_machine,
            commands::install_cli,
            commands::cancel_install,
            commands::open_cli_login,
            commands::optimize_preview,
            commands::apply_optimization,
            commands::set_onboarded,
            commands::set_office_style,
            commands::open_path,
            commands::mission_folder,
            commands::path_info,
            commands::list_pipelines,
            commands::save_pipeline,
            commands::delete_pipeline,
            commands::list_models,
            commands::answer_question,
            commands::list_questions,
            commands::followup_mission,
            commands::agent_work,
            commands::os_auth_info,
            commands::os_reset_password,
            commands::os_login,
            commands::export_formats,
            commands::export_content,
            commands::mission_files,
            commands::create_workdir,
            commands::list_memories,
            commands::save_memory,
            commands::delete_memory,
            commands::list_doc_templates,
            commands::save_doc_template,
            commands::delete_doc_template,
            commands::import_template_file,
        ])
        .run(tauri::generate_context!())
        .expect("lỗi khi chạy ứng dụng");
}
