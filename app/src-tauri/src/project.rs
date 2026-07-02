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
    CreateProjectInput, MediaAsset, ProjectSettings, ProjectSnapshot, ScriptBlock, Take, TrayItem,
    UpdateBlockInput, UpdateProjectSettingsInput, UpdateTrayItemInput,
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
  media_type TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  duration_us INTEGER,
  width INTEGER,
  height INTEGER,
  proxy_relative_path TEXT,
  thumbnail_relative_path TEXT,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tray_items (
  id TEXT PRIMARY KEY,
  block_id TEXT NOT NULL REFERENCES script_blocks(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
  position INTEGER NOT NULL,
  playback_mode TEXT NOT NULL DEFAULT 'narrate_over',
  in_point_us INTEGER NOT NULL DEFAULT 0,
  out_point_us INTEGER,
  loop_mode TEXT NOT NULL DEFAULT 'freeze'
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
CREATE TABLE IF NOT EXISTS aligned_words (
  take_id TEXT NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  word TEXT NOT NULL,
  start_us INTEGER NOT NULL,
  end_us INTEGER NOT NULL,
  matched INTEGER NOT NULL,
  PRIMARY KEY(take_id, position)
);
CREATE TABLE IF NOT EXISTS transcripts (
  take_id TEXT PRIMARY KEY REFERENCES takes(id) ON DELETE CASCADE,
  text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_blocks_position ON script_blocks(position);
CREATE INDEX IF NOT EXISTS idx_tray_block_position ON tray_items(block_id, position);
"#;

pub fn save_recording(recording: crate::recorder::FinishedRecording) -> Result<ProjectSnapshot> {
    let processed_relative_path = format!("recordings/processed/{}.wav", recording.take_id);
    if let Err(error) = crate::audio::enhance_dialogue(
        &recording.project_path.join(&recording.relative_path),
        &recording.project_path.join(&processed_relative_path),
    ) {
        log::warn!("dialogue enhancement failed; preserving clean raw take: {error}");
        fs::copy(
            recording.project_path.join(&recording.relative_path),
            recording.project_path.join(&processed_relative_path),
        )?;
    }
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
    let _ = fs::remove_file(recording.project_path.join("cache/active-recording.json"));
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
    let mut connection = connect(&root)?;
    recover_interrupted(&root, &mut connection)?;
    snapshot(&root, &connection)
}

fn recover_interrupted(root: &Path, connection: &mut Connection) -> Result<()> {
    let metadata_path = root.join("cache/active-recording.json");
    if !metadata_path.is_file() {
        return Ok(());
    }
    let metadata: crate::recorder::RecoveryMetadata =
        serde_json::from_slice(&fs::read(&metadata_path)?)?;
    let already_saved: i64 = connection.query_row(
        "SELECT COUNT(*) FROM takes WHERE id=?1",
        params![metadata.take_id],
        |row| row.get(0),
    )?;
    if already_saved > 0 {
        fs::remove_file(metadata_path)?;
        return Ok(());
    }
    let raw = root.join(&metadata.relative_path);
    if !raw.is_file() || raw.metadata()?.len() <= 44 {
        fs::remove_file(metadata_path)?;
        return Ok(());
    }
    let recovered_relative = format!("recordings/raw/{}-recovered.wav", metadata.take_id);
    let recovered = root.join(&recovered_relative);
    let repaired = crate::tools::command("ffmpeg")
        .args(["-y", "-i"])
        .arg(&raw)
        .args(["-ac", "1", "-ar", "48000", "-c:a", "pcm_s16le"])
        .arg(&recovered)
        .output()
        .context("Could not start FFmpeg to recover the interrupted take")?;
    if !repaired.status.success() {
        bail!("An interrupted recording was found but could not be repaired");
    }
    let reader = hound::WavReader::open(&recovered)?;
    let duration_us =
        i64::from(reader.duration()) * 1_000_000 / i64::from(reader.spec().sample_rate.max(1));
    drop(reader);
    if duration_us < 100_000 {
        fs::remove_file(metadata_path)?;
        return Ok(());
    }
    let processed_relative = format!("recordings/processed/{}-recovered.wav", metadata.take_id);
    if let Err(error) = crate::audio::enhance_dialogue(&recovered, &root.join(&processed_relative))
    {
        log::warn!("recovered take enhancement failed: {error}");
        fs::copy(&recovered, root.join(&processed_relative))?;
    }
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE takes SET selected=0 WHERE block_id=?1",
        params![metadata.block_id],
    )?;
    transaction.execute(
        "INSERT INTO takes(id,block_id,relative_path,processed_relative_path,duration_us,selected,created_at) VALUES(?1,?2,?3,?4,?5,1,?6)",
        params![metadata.take_id,metadata.block_id,recovered_relative,processed_relative,duration_us,metadata.created_at],
    )?;
    if let Some(tray_item_id) = transaction
        .query_row(
            "SELECT id FROM tray_items WHERE block_id=?1 ORDER BY position LIMIT 1",
            params![metadata.block_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        transaction.execute(
            "INSERT INTO presentation_events(id,take_id,event_type,project_time_us,tray_item_id) VALUES(?1,?2,'activate',0,?3)",
            params![Uuid::new_v4().to_string(),metadata.take_id,tray_item_id],
        )?;
    }
    transaction.execute(
        "UPDATE script_blocks SET status='recorded',alignment_stale=1 WHERE id=?1",
        params![metadata.block_id],
    )?;
    transaction.commit()?;
    fs::remove_file(metadata_path)?;
    Ok(())
}

pub fn save_script(project_path: &str, script: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let mut block_query =
        transaction.prepare("SELECT id,text FROM script_blocks ORDER BY position")?;
    let existing = block_query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(block_query);
    let paragraphs = paragraphs(script);
    transaction.execute(
        "UPDATE project SET script = ?1, updated_at = ?2",
        params![script, Utc::now().to_rfc3339()],
    )?;

    for (position, paragraph) in paragraphs.iter().enumerate() {
        if let Some((id, old_text)) = existing.get(position) {
            if old_text != paragraph {
                transaction.execute(
                    "UPDATE script_blocks SET text=?1,alignment_stale=1 WHERE id=?2",
                    params![paragraph, id],
                )?;
            }
        } else {
            transaction.execute(
                "INSERT INTO script_blocks (id, position, text) VALUES (?1, ?2, ?3)",
                params![Uuid::new_v4().to_string(), position as i64, paragraph],
            )?;
        }
    }
    if paragraphs.len() < existing.len() {
        for (id, _) in existing.iter().skip(paragraphs.len()) {
            let used: i64 = transaction.query_row(
                "SELECT (SELECT COUNT(*) FROM takes WHERE block_id=?1) + (SELECT COUNT(*) FROM tray_items WHERE block_id=?1)",
                params![id], |row| row.get(0),
            )?;
            if used > 0 {
                bail!("Remove the media and takes from trailing blocks before deleting them from the script");
            }
            transaction.execute("DELETE FROM script_blocks WHERE id=?1", params![id])?;
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

pub fn update_settings(
    project_path: &str,
    input: UpdateProjectSettingsInput,
) -> Result<ProjectSnapshot> {
    if !(0.0..=1.0).contains(&input.music_volume) {
        bail!("Music volume must be between 0 and 1");
    }
    if !matches!(input.caption_style.as_str(), "clean" | "bold" | "minimal") {
        bail!("Unsupported caption style");
    }
    if !matches!(input.transition_style.as_str(), "cut" | "dissolve") {
        bail!("Unsupported transition style");
    }
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    if let Some(asset_id) = &input.background_music_asset_id {
        let media_type: Option<String> = connection
            .query_row(
                "SELECT media_type FROM media_assets WHERE id=?1",
                params![asset_id],
                |row| row.get(0),
            )
            .optional()?;
        if media_type.as_deref() != Some("audio") {
            bail!("Background music must be an imported audio file");
        }
    }
    connection.execute(
        "UPDATE project SET background_music_asset_id=?1,music_volume=?2,music_ducking=?3,opening_card=?4,opening_title=?5,caption_style=?6,transition_style=?7,updated_at=?8",
        params![
            input.background_music_asset_id,
            input.music_volume,
            i64::from(input.music_ducking),
            i64::from(input.opening_card),
            input.opening_title.trim(),
            input.caption_style,
            input.transition_style,
            Utc::now().to_rfc3339()
        ],
    )?;
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
        if connection
            .query_row(
                "SELECT 1 FROM media_assets WHERE content_hash=?1",
                params![hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some()
        {
            continue;
        }
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
        let asset_id = Uuid::new_v4().to_string();
        let prepared = crate::media::prepare(&root, &asset_id, &relative_path, media_type)?;
        connection.execute(
          "INSERT OR IGNORE INTO media_assets (id,name,relative_path,media_type,content_hash,duration_us,width,height,proxy_relative_path,thumbnail_relative_path,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
          params![asset_id,name,relative_path,media_type,hash,prepared.duration_us,prepared.width,prepared.height,prepared.proxy_relative_path,prepared.thumbnail_relative_path,Utc::now().to_rfc3339()],
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
    let media_type: String = connection
        .query_row(
            "SELECT media_type FROM media_assets WHERE id=?1",
            params![asset_id],
            |row| row.get(0),
        )
        .context("Media asset was not found")?;
    if media_type == "audio" {
        bail!("Audio files cannot be used as visual cues");
    }
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

pub fn trash_asset(project_path: &str, asset_id: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let used: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM tray_items WHERE asset_id=?1",
        params![asset_id],
        |row| row.get(0),
    )?;
    if used > 0 {
        bail!("Remove this asset from every presentation tray before moving it to trash");
    }
    let paths: (String, Option<String>, Option<String>) = transaction
        .query_row(
            "SELECT relative_path,proxy_relative_path,thumbnail_relative_path FROM media_assets WHERE id=?1",
            params![asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("Media asset was not found")?;
    for relative in [Some(paths.0), paths.1, paths.2].into_iter().flatten() {
        move_to_trash(&root, &relative)?;
    }
    transaction.execute(
        "UPDATE project SET background_music_asset_id=NULL WHERE background_music_asset_id=?1",
        params![asset_id],
    )?;
    transaction.execute("DELETE FROM media_assets WHERE id=?1", params![asset_id])?;
    transaction.commit()?;
    snapshot(&root, &connection)
}

pub fn trash_take(project_path: &str, take_id: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let (block_id, raw, processed): (String, String, Option<String>) = transaction
        .query_row(
            "SELECT block_id,relative_path,processed_relative_path FROM takes WHERE id=?1",
            params![take_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("Take was not found")?;
    move_to_trash(&root, &raw)?;
    if let Some(path) = processed {
        move_to_trash(&root, &path)?;
    }
    transaction.execute("DELETE FROM takes WHERE id=?1", params![take_id])?;
    let replacement: Option<String> = transaction
        .query_row(
            "SELECT id FROM takes WHERE block_id=?1 ORDER BY created_at DESC LIMIT 1",
            params![block_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(replacement) = replacement {
        transaction.execute(
            "UPDATE takes SET selected=1 WHERE id=?1",
            params![replacement],
        )?;
        transaction.execute(
            "UPDATE script_blocks SET status='recorded',alignment_stale=1 WHERE id=?1",
            params![block_id],
        )?;
    } else {
        transaction.execute(
            "UPDATE script_blocks SET status='prepared',alignment_stale=0 WHERE id=?1",
            params![block_id],
        )?;
    }
    transaction.commit()?;
    snapshot(&root, &connection)
}

fn move_to_trash(root: &Path, relative: &str) -> Result<()> {
    let source = root.join(relative);
    if !source.is_file() {
        return Ok(());
    }
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("item");
    let destination = root
        .join("trash")
        .join(format!("{}-{name}", Uuid::new_v4()));
    fs::rename(source, destination)?;
    Ok(())
}

pub fn move_tray_item(
    project_path: &str,
    tray_item_id: &str,
    direction: i64,
) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let (block_id, position): (String, i64) = transaction
        .query_row(
            "SELECT block_id, position FROM tray_items WHERE id=?1",
            params![tray_item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("Tray item was not found")?;
    let target = position + direction.signum();
    if target >= 0 {
        if let Some(other_id) = transaction
            .query_row(
                "SELECT id FROM tray_items WHERE block_id=?1 AND position=?2",
                params![block_id, target],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            transaction.execute(
                "UPDATE tray_items SET position=-1 WHERE id=?1",
                params![tray_item_id],
            )?;
            transaction.execute(
                "UPDATE tray_items SET position=?1 WHERE id=?2",
                params![position, other_id],
            )?;
            transaction.execute(
                "UPDATE tray_items SET position=?1 WHERE id=?2",
                params![target, tray_item_id],
            )?;
        }
    }
    transaction.commit()?;
    snapshot(&root, &connection)
}

pub fn move_block(project_path: &str, block_id: &str, direction: i64) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let position: i64 = transaction
        .query_row(
            "SELECT position FROM script_blocks WHERE id=?1",
            params![block_id],
            |row| row.get(0),
        )
        .context("Block was not found")?;
    let target = position + direction.signum();
    if target >= 0 {
        if let Some(other_id) = transaction
            .query_row(
                "SELECT id FROM script_blocks WHERE position=?1",
                params![target],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            transaction.execute(
                "UPDATE script_blocks SET position=-1 WHERE id=?1",
                params![block_id],
            )?;
            transaction.execute(
                "UPDATE script_blocks SET position=?1 WHERE id=?2",
                params![position, other_id],
            )?;
            transaction.execute(
                "UPDATE script_blocks SET position=?1 WHERE id=?2",
                params![target, block_id],
            )?;
            rebuild_script(&transaction)?;
        }
    }
    transaction.commit()?;
    snapshot(&root, &connection)
}

pub fn split_block(
    project_path: &str,
    block_id: &str,
    left_text: &str,
    right_text: &str,
) -> Result<ProjectSnapshot> {
    if left_text.trim().is_empty() || right_text.trim().is_empty() {
        bail!("Both sides of a split must contain text");
    }
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    ensure_block_unused(&transaction, block_id)?;
    let position: i64 = transaction
        .query_row(
            "SELECT position FROM script_blocks WHERE id=?1",
            params![block_id],
            |row| row.get(0),
        )
        .context("Block was not found")?;
    transaction.execute(
        "UPDATE script_blocks SET position=position+1 WHERE position>?1",
        params![position],
    )?;
    transaction.execute(
        "UPDATE script_blocks SET text=?1,alignment_stale=1 WHERE id=?2",
        params![left_text.trim(), block_id],
    )?;
    transaction.execute(
        "INSERT INTO script_blocks(id,position,text) VALUES(?1,?2,?3)",
        params![Uuid::new_v4().to_string(), position + 1, right_text.trim()],
    )?;
    rebuild_script(&transaction)?;
    transaction.commit()?;
    snapshot(&root, &connection)
}

pub fn merge_block_with_next(project_path: &str, block_id: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    ensure_block_unused(&transaction, block_id)?;
    let (position, text): (i64, String) = transaction
        .query_row(
            "SELECT position,text FROM script_blocks WHERE id=?1",
            params![block_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("Block was not found")?;
    let (next_id, next_text): (String, String) = transaction
        .query_row(
            "SELECT id,text FROM script_blocks WHERE position=?1",
            params![position + 1],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("This is already the final block")?;
    ensure_block_unused(&transaction, &next_id)?;
    transaction.execute(
        "UPDATE script_blocks SET text=?1,alignment_stale=1 WHERE id=?2",
        params![format!("{} {}", text.trim(), next_text.trim()), block_id],
    )?;
    transaction.execute("DELETE FROM script_blocks WHERE id=?1", params![next_id])?;
    transaction.execute(
        "UPDATE script_blocks SET position=position-1 WHERE position>?1",
        params![position + 1],
    )?;
    rebuild_script(&transaction)?;
    transaction.commit()?;
    snapshot(&root, &connection)
}

fn ensure_block_unused(connection: &Connection, block_id: &str) -> Result<()> {
    let used: i64 = connection.query_row(
        "SELECT (SELECT COUNT(*) FROM takes WHERE block_id=?1) + (SELECT COUNT(*) FROM tray_items WHERE block_id=?1)",
        params![block_id],
        |row| row.get(0),
    )?;
    if used > 0 {
        bail!("Split or merge blocks before adding media or recording takes");
    }
    Ok(())
}

pub fn select_take(project_path: &str, take_id: &str) -> Result<ProjectSnapshot> {
    let root = project_root(project_path)?;
    let mut connection = connect(&root)?;
    let transaction = connection.transaction()?;
    let block_id: String = transaction
        .query_row(
            "SELECT block_id FROM takes WHERE id=?1",
            params![take_id],
            |row| row.get(0),
        )
        .context("Take was not found")?;
    transaction.execute(
        "UPDATE takes SET selected=0 WHERE block_id=?1",
        params![block_id],
    )?;
    transaction.execute("UPDATE takes SET selected=1 WHERE id=?1", params![take_id])?;
    transaction.execute(
        "UPDATE script_blocks SET alignment_stale=1 WHERE id=?1",
        params![block_id],
    )?;
    transaction.commit()?;
    snapshot(&root, &connection)
}

pub fn update_tray_item(project_path: &str, item: UpdateTrayItemInput) -> Result<ProjectSnapshot> {
    if !matches!(item.playback_mode.as_str(), "narrate_over" | "play_solo") {
        bail!("Unsupported playback mode");
    }
    if !matches!(item.loop_mode.as_str(), "freeze" | "repeat") {
        bail!("Unsupported loop mode");
    }
    if item.in_point_us < 0 || item.out_point_us.is_some_and(|out| out <= item.in_point_us) {
        bail!("Invalid media range");
    }
    let root = project_root(project_path)?;
    let connection = connect(&root)?;
    let duration_us: Option<i64> = connection
        .query_row(
            "SELECT a.duration_us FROM tray_items t JOIN media_assets a ON a.id=t.asset_id WHERE t.id=?1",
            params![item.id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if duration_us.is_some_and(|duration| {
        item.in_point_us >= duration || item.out_point_us.is_some_and(|out| out > duration)
    }) {
        bail!("Trim range exceeds the source duration");
    }
    let changed = connection.execute(
        "UPDATE tray_items SET playback_mode=?1,in_point_us=?2,out_point_us=?3,loop_mode=?4 WHERE id=?5",
        params![
            item.playback_mode,
            item.in_point_us,
            item.out_point_us,
            item.loop_mode,
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
    for migration in [
        "ALTER TABLE media_assets ADD COLUMN duration_us INTEGER",
        "ALTER TABLE media_assets ADD COLUMN width INTEGER",
        "ALTER TABLE media_assets ADD COLUMN height INTEGER",
        "ALTER TABLE media_assets ADD COLUMN proxy_relative_path TEXT",
        "ALTER TABLE media_assets ADD COLUMN thumbnail_relative_path TEXT",
        "ALTER TABLE tray_items ADD COLUMN loop_mode TEXT NOT NULL DEFAULT 'freeze'",
        "ALTER TABLE project ADD COLUMN background_music_asset_id TEXT",
        "ALTER TABLE project ADD COLUMN music_volume REAL NOT NULL DEFAULT 0.18",
        "ALTER TABLE project ADD COLUMN music_ducking INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE project ADD COLUMN opening_card INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE project ADD COLUMN opening_title TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE project ADD COLUMN caption_style TEXT NOT NULL DEFAULT 'clean'",
        "ALTER TABLE project ADD COLUMN transition_style TEXT NOT NULL DEFAULT 'cut'",
    ] {
        let _ = connection.execute(migration, []);
    }
    Ok(connection)
}

fn snapshot(root: &Path, connection: &Connection) -> Result<ProjectSnapshot> {
    let project = connection
        .query_row(
            "SELECT id,name,aspect_ratio,platform_target,script,background_music_asset_id,music_volume,music_ducking,opening_card,opening_title,caption_style,transition_style FROM project LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get::<_, i64>(7)? != 0,
                    row.get::<_, i64>(8)? != 0,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                ))
            },
        )
        .optional()?
        .context("This folder does not contain a Cheeza project")?;

    let mut asset_statement = connection.prepare("SELECT id,name,relative_path,media_type,content_hash,duration_us,width,height,proxy_relative_path,thumbnail_relative_path FROM media_assets ORDER BY created_at")?;
    let assets = asset_statement
        .query_map([], |row| {
            Ok(MediaAsset {
                id: row.get(0)?,
                name: row.get(1)?,
                relative_path: row.get(2)?,
                media_type: row.get(3)?,
                content_hash: row.get(4)?,
                duration_us: row.get(5)?,
                width: row.get(6)?,
                height: row.get(7)?,
                proxy_relative_path: row.get(8)?,
                thumbnail_relative_path: row.get(9)?,
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
                takes: load_takes(connection, &id)?,
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
        settings: ProjectSettings {
            background_music_asset_id: project.5,
            music_volume: project.6,
            music_ducking: project.7,
            opening_card: project.8,
            opening_title: project.9,
            caption_style: project.10,
            transition_style: project.11,
        },
        blocks,
        assets,
    })
}

fn load_takes(connection: &Connection, block_id: &str) -> rusqlite::Result<Vec<Take>> {
    let mut statement = connection.prepare("SELECT t.id,t.relative_path,t.processed_relative_path,t.duration_us,t.selected,t.created_at,(SELECT COUNT(*) FROM aligned_words w WHERE w.take_id=t.id),(SELECT COUNT(*) FROM aligned_words w WHERE w.take_id=t.id AND w.matched=1),(SELECT text FROM transcripts x WHERE x.take_id=t.id) FROM takes t WHERE t.block_id=?1 ORDER BY t.created_at DESC")?;
    let takes = statement
        .query_map(params![block_id], |row| {
            Ok(Take {
                id: row.get(0)?,
                relative_path: row.get(1)?,
                processed_relative_path: row.get(2)?,
                duration_us: row.get(3)?,
                selected: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                alignment_total: row.get(6)?,
                alignment_matched: row.get(7)?,
                transcript: row.get(8)?,
            })
        })?
        .collect();
    takes
}

fn load_tray(connection: &Connection, block_id: &str) -> rusqlite::Result<Vec<TrayItem>> {
    let mut statement = connection.prepare(
    "SELECT id,asset_id,position,playback_mode,in_point_us,out_point_us,loop_mode FROM tray_items WHERE block_id=?1 ORDER BY position",
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
                loop_mode: row.get(6)?,
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
    use super::{
        create, media_type, merge_block_with_next, move_block, move_to_trash, paragraphs,
        safe_folder_name, save_script, split_block,
    };
    use crate::models::CreateProjectInput;

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

    #[test]
    fn script_edits_reconcile_and_blocks_reorder() {
        let temp = tempfile::tempdir().unwrap();
        let project = create(CreateProjectInput {
            parent_path: temp.path().to_string_lossy().into_owned(),
            name: "Script test".into(),
            aspect_ratio: "9:16".into(),
            platform_target: "TikTok".into(),
        })
        .unwrap();
        let project = save_script(&project.path, "First block.\n\nSecond block.").unwrap();
        let first_id = project.blocks[0].id.clone();
        let project = save_script(
            &project.path,
            "Edited first block.\n\nSecond block.\n\nThird block.",
        )
        .unwrap();
        assert_eq!(project.blocks.len(), 3);
        assert_eq!(project.blocks[0].id, first_id);
        assert!(project.blocks[0].alignment_stale);
        let project = move_block(&project.path, &first_id, 1).unwrap();
        assert_eq!(project.blocks[1].id, first_id);
        assert_eq!(
            project.script,
            "Second block.\n\nEdited first block.\n\nThird block."
        );
        let third_id = project.blocks[2].id.clone();
        let project = split_block(&project.path, &third_id, "Third", "block.").unwrap();
        assert_eq!(project.blocks.len(), 4);
        assert_eq!(project.blocks[2].text, "Third");
        let project = merge_block_with_next(&project.path, &third_id).unwrap();
        assert_eq!(project.blocks.len(), 3);
        assert_eq!(project.blocks[2].text, "Third block.");
    }

    #[test]
    fn moves_files_to_project_trash() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("trash")).unwrap();
        std::fs::create_dir(temp.path().join("assets")).unwrap();
        std::fs::write(temp.path().join("assets/example.png"), b"fixture").unwrap();
        move_to_trash(temp.path(), "assets/example.png").unwrap();
        assert!(!temp.path().join("assets/example.png").exists());
        assert_eq!(
            std::fs::read_dir(temp.path().join("trash"))
                .unwrap()
                .count(),
            1
        );
    }
}
