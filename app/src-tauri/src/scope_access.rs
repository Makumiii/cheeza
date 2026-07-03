use std::path::Path;

use tauri::{AppHandle, Manager};

pub fn allow_project_media(app: &AppHandle, project_path: &str) -> Result<(), String> {
    let path = Path::new(project_path);
    if !path.is_dir() {
        return Err(format!("Project folder not found: {project_path}"));
    }

    app.asset_protocol_scope()
        .allow_directory(path, true)
        .map_err(|error| format!("Could not enable media access for project: {error}"))?;

    Ok(())
}

pub fn allow_media_path(app: &AppHandle, media_path: &str) -> Result<(), String> {
    let path = Path::new(media_path);
    if path.is_file() {
        app.asset_protocol_scope()
            .allow_file(path)
            .map_err(|error| format!("Could not enable media access: {error}"))?;
        return Ok(());
    }

    if path.is_dir() {
        return allow_project_media(app, media_path);
    }

    Err(format!("Media path not found: {media_path}"))
}
