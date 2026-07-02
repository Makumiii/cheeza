mod models;
mod project;

use models::{CreateProjectInput, ProjectSnapshot, UpdateBlockInput};

#[tauri::command]
fn create_project(input: CreateProjectInput) -> Result<ProjectSnapshot, String> {
    project::create(input).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_project(project_path: String) -> Result<ProjectSnapshot, String> {
    project::open(&project_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_script(project_path: String, script: String) -> Result<ProjectSnapshot, String> {
    project::save_script(&project_path, &script).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_block(project_path: String, block: UpdateBlockInput) -> Result<ProjectSnapshot, String> {
    project::update_block(&project_path, block).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_media(
    project_path: String,
    source_paths: Vec<String>,
) -> Result<ProjectSnapshot, String> {
    project::import_media(&project_path, &source_paths).map_err(|error| error.to_string())
}

#[tauri::command]
fn add_tray_item(
    project_path: String,
    block_id: String,
    asset_id: String,
) -> Result<ProjectSnapshot, String> {
    project::add_tray_item(&project_path, &block_id, &asset_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_tray_item(project_path: String, tray_item_id: String) -> Result<ProjectSnapshot, String> {
    project::remove_tray_item(&project_path, &tray_item_id).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_project,
            open_project,
            save_script,
            update_block,
            import_media,
            add_tray_item,
            remove_tray_item
        ])
        .run(tauri::generate_context!())
        .expect("error while running Cheeza");
}
