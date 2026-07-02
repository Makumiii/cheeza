use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    CreateProjectInput, MediaAsset, ProjectSnapshot, ScriptBlock, TrayItem, UpdateBlockInput,
    UpdateTrayItemInput,
};

const DB_NAME: &str = "cheeza.sqlite";
const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS project (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  aspect_ratio TEXT NOT NULL,
  platform_target TEXT NOT NULL,
  script TEXT NOT NULL DEFAULT '',
  schema_version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS script_blocks (
  id TEXT PRIMARY KEY,
  position INTEGER NOT NULL,
  text TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'prepared',
  alignment_stale INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS media_assets (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  relative_path TEXT NOT NULL UNIQUE,
  processed_relative_path TEXT,
  media_type TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tray_items (
  id TEXT PRIMARY KEY,
  block_id TEXT NOT NULL REFERENCES script_blocks(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
  position INTEGER NOT NULL,
  playback_mode TEXT NOT NULL DEFAULT 'narrate_over',
  in_point_us INTEGER NOT NULL DEFAULT 0,
  out_point_us INTEGER
);
CREATE TABLE IF NOT EXISTS takes (
  id TEXT PRIMARY KEY,
  block_id TEXT NOT NULL REFERENCES script_blocks(id) ON DELETE CASCADE,
  relative_path TEXT NOT NULL UNIQUE,
  duration_us INTEGER NOT NULL,
  selected INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS presentation_events (
  id TEXT PRIMARY KEY,
  take_id TEXT NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  project_time_us INTEGER NOT NULL,
  tray_item_id TEXT REFERENCES tray_items(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_blocks_position ON script_blocks(position);
CREATE INDEX IF NOT EXISTS idx_tray_block_position ON tray_items(block_id, position);
"#;

pub fn save_recording(recording: crate::recorder::FinishedRecording) -> Result<ProjectSnapshot> {
    let processed_relative_path = format!("recordings/processed/{}.wav", recording.take_id);
    crate::audio::enhance_dialogue(
        &recording.project_path.join(&recording.relative_path),
        &recording.project_path.join(&processed_relative_path),
    )?;
    let mut connection = connect(&recording.project_path)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE takes SET selected = 0 WHERE block_id = ?1",
        params![recording.block_id],
    )?;
    transaction.execute(
        "INSERT INTO takes (id, block_id, relative_path, processed_relative_path, duration_us, selected, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
        params![recording.take_id, recording.block_id, recording.relative_path, processed_relative_path, recording.duration_us, Utc::now().to_rfc3339()],
    )?;
    for event in recording.events {
        transaction.execute(
            "INSERT INTO presentation_events (id, take_id, event_type, project_time_us, tray_item_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), recording.take_id, event.event_type, event.project_time_us, event.tray_item_id],
        )?;
    }
    transaction.execute(
        "UPDATE script_blocks SET status = 'recorded', alignment_stale = 1 WHERE id = ?1",
        params![recording.block_id],
    )?;
    transaction.commit()?;
    snapshot(&recording.project_path, &connection)
}

pub fn create(input: CreateProjectInput) -> Result<ProjectSnapshot> {
    let name = input.name.trim();
    if name.is_empty() {
        bail!("Project name is required");
    }
    if !matches!(input.aspect_ratio.as_str(), "9:16" | "16:9") {
        bail!("Unsupported aspect ratio");
    }

    let folder_name = safe_folder_name(name);
    let root = PathBuf::from(&input.parent_path).join(folder_name);
    if root.exists() {
        bail!("A project folder with this name already exists");
    }

    for directory in [
        "assets/originals",
        "assets/proxies",
        "assets/thumbnails",
        "recordings/raw",
        "recordings/processed",
        "captions",
        "cache",
        "exports",
        "trash",
    ] {
        fs::create_dir_all(root.join(directory))?;
    }

    let connection = connect(&root)?;
    let now = Utc::now().to_rfc3339();
    connection.execute(
    "INSERT INTO project (id, name, aspect_ratio, platform_target, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    params![Uuid::new_v4().to_string(), name, input.aspect_ratio, input.platform_target, now],
  )?;
    snapshot(&root, &connection)
}

pub fn open(project_path: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    snapshot(&root, &connection)
}

pub fn save_script(project_path: &str, script: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let count: i64 =
        transaction.query_row("SELECT COUNT(*) FROM script_blocks", [], |row| row.get(0))?;
    transaction.execute(
        "UPDATE project SET script = ?1, updated_at = ?2",
        params![script, Utc::now().to_rfc3339()],
    )?;

    if count == 0 {
        for (position, paragraph) in paragraphs(script).into_iter().enumerate() {
            transaction.execute(
                "INSERT INTO script_blocks (id, position, text) VALUES (?1, ?2, ?3)",
                params![Uuid::new_v4().to_string(), position as i64, paragraph],
            )?;
        }
    }
    transaction.commit()?;
    snapshot(&root, &connection)
}

pub fn update_block(project_path: &str, block: UpdateBlockInput) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    let changed = connection.execute(
        "UPDATE script_blocks SET text = ?1, alignment_stale = 1 WHERE id = ?2",
        params![block.text.trim(), block.id],
    )?;
    if changed == 0 {
        bail!("Script block was not found");
    }
    rebuild_script(&connection)?;
    snapshot(&root, &connection)
}

pub fn import_media(project_path: &str, source_paths: &[String]) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    for source in source_paths {
        let source = Path::new(source);
        if !source.is_file() {
            bail!("Media file does not exist: {}", source.display());
        }
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let media_type = media_type(&extension).context("Unsupported media format")?;
        let hash = hash_file(source)?;
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .context("Invalid media filename")?;
        let destination_name = unique_asset_name(&hash, name);
        let relative_path = format!("assets/originals/{destination_name}");
        let destination = root.join(&relative_path);
        if !destination.exists() {
            fs::copy(source, &destination)?;
        }
        connection.execute(
      "INSERT OR IGNORE INTO media_assets (id, name, relative_path, media_type, content_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
      params![Uuid::new_v4().to_string(), name, relative_path, media_type, hash, Utc::now().to_rfc3339()],
    )?;
    }
    snapshot(&root, &connection)
}

pub fn add_tray_item(
    project_path: &str,
    block_id: &str,
    asset_id: &str,
) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    let position: i64 = connection.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM tray_items WHERE block_id = ?1",
        params![block_id],
        |row| row.get(0),
    )?;
    connection.execute(
        "INSERT INTO tray_items (id, block_id, asset_id, position) VALUES (?1, ?2, ?3, ?4)",
        params![Uuid::new_v4().to_string(), block_id, asset_id, position],
    )?;
    snapshot(&root, &connection)
}

pub fn remove_tray_item(project_path: &str, tray_item_id: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    connection.execute(
        "DELETE FROM tray_items WHERE id = ?1",
        params![tray_item_id],
    )?;
    snapshot(&root, &connection)
}

pub fn update_tray_item(project_path: &str, item: UpdateTrayItemInput) -> Result<ProjectSnapshot> {
    if !matches!(item.playback_mode.as_str(), "narrate_over" | "play_solo") {
        bail!("Unsupported playback mode");
    }
    if item.in_point_us < 0 || item.out_point_us.is_some_and(|out| out <= item.in_point_us) {
        bail!("Invalid media range");
    }
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    let changed = connection.execute(
        "UPDATE tray_items SET playback_mode=?1,in_point_us=?2,out_point_us=?3 WHERE id=?4",
        params![
            item.playback_mode,
            item.in_point_us,
            item.out_point_us,
            item.id
        ],
    )?;
    if changed == 0 {
        bail!("Tray item was not found");
    }
    snapshot(&root, &connection)
}

fn connect(root: &Path) -> Result<Connection> {
    let connection = Connection::open(root.join(DB_NAME))?;
    connection.execute_batch(SCHEMA)?;
    let _ = connection.execute(
        "ALTER TABLE takes ADD COLUMN processed_relative_path TEXT",
        [],
    );
    Ok(connection)
}

fn snapshot(root: &Path, connection: &Connection) -> Result<ProjectSnapshot> {
    let project = connection
        .query_row(
            "SELECT id, name, aspect_ratio, platform_target, script FROM project LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .context("This folder does not contain a Cheeza project")?;

    let mut asset_statement = connection.prepare("SELECT id, name, relative_path, media_type, content_hash FROM media_assets ORDER BY created_at")?;
    let assets = asset_statement
        .query_map([], |row| {
            Ok(MediaAsset {
                id: row.get(0)?,
                name: row.get(1)?,
                relative_path: row.get(2)?,
                media_type: row.get(3)?,
                content_hash: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut block_statement = connection.prepare(
        "SELECT id, position, text, status, alignment_stale FROM script_blocks ORDER BY position",
    )?;
    let blocks = block_statement
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(ScriptBlock {
                tray: load_tray(connection, &id)?,
                id,
                position: row.get(1)?,
                text: row.get(2)?,
                status: row.get(3)?,
                alignment_stale: row.get::<_, i64>(4)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ProjectSnapshot {
        id: project.0,
        name: project.1,
        path: root.to_string_lossy().into_owned(),
        aspect_ratio: project.2,
        platform_target: project.3,
        script: project.4,
        blocks,
        assets,
    })
}

fn load_tray(connection: &Connection, block_id: &str) -> rusqlite::Result<Vec<TrayItem>> {
    let mut statement = connection.prepare(
    "SELECT id, asset_id, position, playback_mode, in_point_us, out_point_us FROM tray_items WHERE block_id = ?1 ORDER BY position",
  )?;
    let items = statement
        .query_map(params![block_id], |row| {
            Ok(TrayItem {
                id: row.get(0)?,
                asset_id: row.get(1)?,
                position: row.get(2)?,
                playback_mode: row.get(3)?,
                in_point_us: row.get(4)?,
                out_point_us: row.get(5)?,
            })
        })?
        .collect();
    items
}

fn rebuild_script(connection: &Connection) -> Result<()> {
    let mut statement = connection.prepare("SELECT text FROM script_blocks ORDER BY position")?;
    let parts = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    connection.execute(
        "UPDATE project SET script = ?1, updated_at = ?2",
        params![parts.join("\n\n"), Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn project_root(project_path: &str) -> Result<PathBuf> {
    let root = PathBuf::from(project_path);
    if !root.join(DB_NAME).is_file() {
        bail!("Cheeza project database was not found");
    }
    Ok(root)
}

fn paragraphs(script: &str) -> Vec<String> {
    script
        .split("\n\n")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn safe_folder_name(name: &str) -> String {
    let value: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    value.trim_matches('-').to_ascii_lowercase()
}

fn media_type(extension: &str) -> Option<&'static str> {
    match extension {
        "jpg" | "jpeg" | "png" | "webp" | "gif" => Some("image"),
        "mp4" | "mov" | "m4v" | "webm" => Some("video"),
        "wav" | "mp3" | "m4a" | "aac" | "flac" | "ogg" | "opus" => Some("audio"),
        _ => None,
    }
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn unique_asset_name(hash: &str, name: &str) -> String {
    format!("{}-{name}", &hash[..12])
}

#[cfg(test)]
mod tests {
    use super::{media_type, paragraphs, safe_folder_name};

    #[test]
    fn splits_paragraphs_without_empty_blocks() {
        assert_eq!(
            paragraphs(" First.\n\n\n Second. \n\n"),
            ["First.", "Second."]
        );
    }

    #[test]
    fn creates_safe_project_folder_names() {
        assert_eq!(safe_folder_name("My First: Video!"), "my-first--video");
    }

    #[test]
    fn official_formats_are_classified() {
        assert_eq!(media_type("mp4"), Some("video"));
        assert_eq!(media_type("png"), Some("image"));
        assert_eq!(media_type("exe"), None);
    }
}
