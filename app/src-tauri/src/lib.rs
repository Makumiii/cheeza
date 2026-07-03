mod audio;
mod captions;
mod media;
mod models;
mod project;
mod recorder;
mod render;
mod scope_access;
mod speech;
mod tools;

use models::{
    CreateProjectInput, ProjectSnapshot, UpdateBlockInput, UpdateProjectSettingsInput,
    UpdateTrayItemInput,
};
use recorder::{RecordingState, RecordingStatus};

#[tauri::command]
fn create_project(
    app: tauri::AppHandle,
    input: CreateProjectInput,
) -> Result<ProjectSnapshot, String> {
    let snapshot = project::create(input).map_err(|error| error.to_string())?;
    scope_access::allow_project_media(&app, &snapshot.path)?;
    Ok(snapshot)
}
#[tauri::command]
fn open_project(app: tauri::AppHandle, project_path: String) -> Result<ProjectSnapshot, String> {
    scope_access::allow_project_media(&app, &project_path)?;
    project::open(&project_path).map_err(|error| error.to_string())
}
#[tauri::command]
fn save_script(project_path: String, script: String) -> Result<ProjectSnapshot, String> {
    project::save_script(&project_path, &script).map_err(|error| error.to_string())
}
#[tauri::command]
fn read_script_file(path: String) -> Result<String, String> {
    let file = std::path::Path::new(&path);
    if file
        .extension()
        .and_then(|value| value.to_str())
        .map_or(true, |value| !value.eq_ignore_ascii_case("txt"))
    {
        return Err("Choose a plain-text .txt script".into());
    }
    std::fs::read_to_string(file).map_err(|error| format!("Could not read script: {error}"))
}
#[tauri::command]
fn update_block(project_path: String, block: UpdateBlockInput) -> Result<ProjectSnapshot, String> {
    project::update_block(&project_path, block).map_err(|error| error.to_string())
}
#[tauri::command]
fn update_project_settings(
    project_path: String,
    input: UpdateProjectSettingsInput,
) -> Result<ProjectSnapshot, String> {
    project::update_settings(&project_path, input).map_err(|error| error.to_string())
}
#[tauri::command]
fn import_media(
    app: tauri::AppHandle,
    project_path: String,
    source_paths: Vec<String>,
) -> Result<ProjectSnapshot, String> {
    scope_access::allow_project_media(&app, &project_path)?;
    let snapshot =
        project::import_media(&project_path, &source_paths).map_err(|error| error.to_string())?;
    scope_access::allow_project_media(&app, &snapshot.path)?;
    Ok(snapshot)
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
fn trash_asset(project_path: String, asset_id: String) -> Result<ProjectSnapshot, String> {
    project::trash_asset(&project_path, &asset_id).map_err(|error| error.to_string())
}
#[tauri::command]
fn trash_take(project_path: String, take_id: String) -> Result<ProjectSnapshot, String> {
    project::trash_take(&project_path, &take_id).map_err(|error| error.to_string())
}
#[tauri::command]
fn update_tray_item(
    project_path: String,
    item: UpdateTrayItemInput,
) -> Result<ProjectSnapshot, String> {
    project::update_tray_item(&project_path, item).map_err(|error| error.to_string())
}
#[tauri::command]
fn move_tray_item(
    project_path: String,
    tray_item_id: String,
    direction: i64,
) -> Result<ProjectSnapshot, String> {
    project::move_tray_item(&project_path, &tray_item_id, direction)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn move_block(
    project_path: String,
    block_id: String,
    direction: i64,
) -> Result<ProjectSnapshot, String> {
    project::move_block(&project_path, &block_id, direction).map_err(|error| error.to_string())
}
#[tauri::command]
fn split_block(
    project_path: String,
    block_id: String,
    left_text: String,
    right_text: String,
) -> Result<ProjectSnapshot, String> {
    project::split_block(&project_path, &block_id, &left_text, &right_text)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn merge_block_with_next(
    project_path: String,
    block_id: String,
) -> Result<ProjectSnapshot, String> {
    project::merge_block_with_next(&project_path, &block_id).map_err(|error| error.to_string())
}
#[tauri::command]
fn select_take(project_path: String, take_id: String) -> Result<ProjectSnapshot, String> {
    project::select_take(&project_path, &take_id).map_err(|error| error.to_string())
}
#[tauri::command]
fn list_input_devices() -> Result<Vec<String>, String> {
    recorder::input_devices().map_err(|error| error.to_string())
}
#[tauri::command]
fn sound_check(device_name: Option<String>) -> Result<recorder::SoundCheck, String> {
    recorder::sound_check(device_name.as_deref()).map_err(|error| error.to_string())
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
fn recording_status(state: tauri::State<'_, RecordingState>) -> Result<RecordingStatus, String> {
    let current = state.0.lock();
    Ok(current.as_ref().ok_or("No recording is active")?.status())
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
fn start_media_break(state: tauri::State<'_, RecordingState>) -> Result<RecordingStatus, String> {
    let mut current = state.0.lock();
    let recording = current.as_mut().ok_or("No recording is active")?;
    recording.start_media_break();
    Ok(recording.status())
}

#[tauri::command]
fn end_media_break(state: tauri::State<'_, RecordingState>) -> Result<RecordingStatus, String> {
    let mut current = state.0.lock();
    let recording = current.as_mut().ok_or("No recording is active")?;
    recording.end_media_break();
    Ok(recording.status())
}

#[tauri::command]
fn stop_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, RecordingState>,
) -> Result<ProjectSnapshot, String> {
    let recording = state.0.lock().take().ok_or("No recording is active")?;
    let snapshot = recording
        .finish()
        .and_then(project::save_recording)
        .map_err(|error| error.to_string())?;
    scope_access::allow_project_media(&app, &snapshot.path)?;
    Ok(snapshot)
}

#[tauri::command]
fn export_project(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<render::ExportResult, String> {
    scope_access::allow_project_media(&app, &project_path)?;
    let result = render::export(&project_path).map_err(|error| error.to_string())?;
    scope_access::allow_media_path(&app, &result.path)?;
    Ok(result)
}
#[tauri::command]
fn preview_project(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<render::ExportResult, String> {
    scope_access::allow_project_media(&app, &project_path)?;
    let result = render::preview(&project_path).map_err(|error| error.to_string())?;
    scope_access::allow_media_path(&app, &result.path)?;
    Ok(result)
}

#[tauri::command]
fn align_block(project_path: String, block_id: String) -> Result<Vec<speech::AlignedWord>, String> {
    speech::align_block(&project_path, &block_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn ensure_project_media_access(app: tauri::AppHandle, project_path: String) -> Result<(), String> {
    scope_access::allow_project_media(&app, &project_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RecordingState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_persisted_scope::init())
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
            read_script_file,
            update_block,
            update_project_settings,
            import_media,
            add_tray_item,
            remove_tray_item,
            trash_asset,
            trash_take,
            update_tray_item,
            move_tray_item,
            move_block,
            split_block,
            merge_block_with_next,
            select_take,
            list_input_devices,
            sound_check,
            start_recording,
            pause_recording,
            resume_recording,
            recording_status,
            record_cue,
            start_media_break,
            end_media_break,
            stop_recording,
            export_project,
            preview_project,
            align_block,
            ensure_project_media_access
        ])
        .run(tauri::generate_context!())
        .expect("error while running Cheeza");
}
