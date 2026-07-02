mod models;
mod project;
mod recorder;
mod render;

use models::{CreateProjectInput, ProjectSnapshot, UpdateBlockInput};
use recorder::{RecordingState, RecordingStatus};

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
#[tauri::command]
fn list_input_devices() -> Result<Vec<String>, String> {
    recorder::input_devices().map_err(|error| error.to_string())
}

#[tauri::command]
fn start_recording(
    state: tauri::State<'_, RecordingState>,
    project_path: String,
    block_id: String,
    device_name: Option<String>,
) -> Result<RecordingStatus, String> {
    let mut current = state.0.lock();
    if current.is_some() {
        return Err("A recording is already active".into());
    }
    let recording = recorder::start(&project_path, &block_id, device_name.as_deref())
        .map_err(|error| error.to_string())?;
    let status = recording.status();
    *current = Some(recording);
    Ok(status)
}

#[tauri::command]
fn pause_recording(state: tauri::State<'_, RecordingState>) -> Result<RecordingStatus, String> {
    let mut current = state.0.lock();
    let recording = current.as_mut().ok_or("No recording is active")?;
    recording.pause();
    Ok(recording.status())
}

#[tauri::command]
fn resume_recording(state: tauri::State<'_, RecordingState>) -> Result<RecordingStatus, String> {
    let mut current = state.0.lock();
    let recording = current.as_mut().ok_or("No recording is active")?;
    recording.resume();
    Ok(recording.status())
}

#[tauri::command]
fn record_cue(
    state: tauri::State<'_, RecordingState>,
    event_type: String,
    tray_item_id: Option<String>,
) -> Result<RecordingStatus, String> {
    let mut current = state.0.lock();
    let recording = current.as_mut().ok_or("No recording is active")?;
    recording.cue(&event_type, tray_item_id);
    Ok(recording.status())
}

#[tauri::command]
fn stop_recording(state: tauri::State<'_, RecordingState>) -> Result<ProjectSnapshot, String> {
    let recording = state.0.lock().take().ok_or("No recording is active")?;
    recording
        .finish()
        .and_then(project::save_recording)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_project(project_path: String) -> Result<render::ExportResult, String> {
    render::export(&project_path).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RecordingState::default())
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
            remove_tray_item,
            list_input_devices,
            start_recording,
            pause_recording,
            resume_recording,
            record_cue,
            stop_recording,
            export_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running Cheeza");
}
