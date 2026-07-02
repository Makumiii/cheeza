use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub duration_us: i64,
}
struct BlockRender {
    text: String,
    audio: PathBuf,
    duration_us: i64,
    cues: Vec<Cue>,
    words: Vec<crate::captions::TimedWord>,
}
struct Cue {
    path: PathBuf,
    media_type: String,
    start_us: i64,
    in_point_us: i64,
    playback_mode: String,
}

pub fn export(project_path: &str) -> Result<ExportResult> {
    ensure_ffmpeg()?;
    let root = PathBuf::from(project_path);
    let db = Connection::open(root.join("cheeza.sqlite"))?;
    let (name, aspect): (String, String) = db.query_row(
        "SELECT name, aspect_ratio FROM project LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let blocks = load_blocks(&db, &root)?;
    if blocks.is_empty() {
        bail!("No recorded blocks are available to export");
    }
    if blocks.iter().any(|block| block.cues.is_empty()) {
        bail!("Every recorded block needs at least one presentation cue");
    }
    let work = root.join("cache/export-work");
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir_all(&work)?;
    let dimensions = if aspect == "9:16" {
        "1080:1920"
    } else {
        "1920:1080"
    };
    let mut rendered = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        rendered.push(render_block(block, &work, index, dimensions)?);
    }
    let assembled = work.join("assembled.mp4");
    concat(&rendered, &work.join("blocks.txt"), &assembled, true)?;
    let caption_path = root.join("captions/captions.srt");
    let mut offset_us = 0;
    let captions = blocks
        .iter()
        .map(|block| {
            let item = crate::captions::CaptionBlock {
                text: &block.text,
                offset_us,
                duration_us: block.duration_us,
            };
            offset_us += block.duration_us;
            item
        })
        .collect::<Vec<_>>();
    crate::captions::write_srt(&caption_path, &captions)?;
    if blocks.iter().all(|block| !block.words.is_empty()) {
        let mut aligned_offset = 0;
        let aligned = blocks
            .iter()
            .map(|block| {
                let item = crate::captions::AlignedBlock {
                    offset_us: aligned_offset,
                    words: block.words.clone(),
                };
                aligned_offset += block.duration_us;
                item
            })
            .collect::<Vec<_>>();
        crate::captions::write_aligned_srt(&caption_path, &aligned)?;
    }
    let output = root.join("exports").join(format!(
        "{}-{aspect}-{}.mp4",
        safe_name(&name),
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let subtitle = caption_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    run(Command::new("ffmpeg").args(["-y", "-i"]).arg(&assembled).args(["-vf", &format!("subtitles='{subtitle}':force_style='FontName=Arial,FontSize=18,PrimaryColour=&H00FFFFFF,OutlineColour=&H00101010,Outline=3,Shadow=0,Alignment=2,MarginV=110'"), "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-c:a", "copy", "-movflags", "+faststart"]).arg(&output))?;
    Ok(ExportResult {
        path: output.to_string_lossy().into_owned(),
        duration_us: blocks.iter().map(|block| block.duration_us).sum(),
    })
}

fn load_blocks(db: &Connection, root: &Path) -> Result<Vec<BlockRender>> {
    let mut statement = db.prepare("SELECT COALESCE(t.processed_relative_path,t.relative_path),t.duration_us,t.id,b.text FROM script_blocks b JOIN takes t ON t.block_id=b.id AND t.selected=1 ORDER BY b.position")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut blocks = Vec::new();
    for row in rows {
        let (audio, duration_us, take_id, text) = row?;
        let mut cues_query = db.prepare("SELECT a.relative_path,a.media_type,e.project_time_us,ti.in_point_us,ti.playback_mode FROM presentation_events e JOIN tray_items ti ON ti.id=e.tray_item_id JOIN media_assets a ON a.id=ti.asset_id WHERE e.take_id=?1 AND e.event_type='activate' ORDER BY e.project_time_us")?;
        let cues = cues_query
            .query_map(params![take_id], |row| {
                Ok(Cue {
                    path: root.join(row.get::<_, String>(0)?),
                    media_type: row.get(1)?,
                    start_us: row.get(2)?,
                    in_point_us: row.get(3)?,
                    playback_mode: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut words_query = db.prepare(
            "SELECT word,start_us,end_us FROM aligned_words WHERE take_id=?1 ORDER BY position",
        )?;
        let words = words_query
            .query_map(params![take_id], |row| {
                Ok(crate::captions::TimedWord {
                    word: row.get(0)?,
                    start_us: row.get(1)?,
                    end_us: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        blocks.push(BlockRender {
            text,
            audio: root.join(audio),
            duration_us,
            cues,
            words,
        });
    }
    Ok(blocks)
}

fn render_block(
    block: &BlockRender,
    work: &Path,
    index: usize,
    dimensions: &str,
) -> Result<PathBuf> {
    let mut segments = Vec::new();
    for (cue_index, cue) in block.cues.iter().enumerate() {
        let end = block
            .cues
            .get(cue_index + 1)
            .map_or(block.duration_us, |next| next.start_us);
        let path = work.join(format!("b{index}-c{cue_index}.mp4"));
        render_cue(cue, (end - cue.start_us).max(33_334), dimensions, &path)?;
        segments.push(path);
    }
    let visual = work.join(format!("b{index}-visual.mp4"));
    concat(
        &segments,
        &work.join(format!("b{index}.txt")),
        &visual,
        false,
    )?;
    let output = work.join(format!("block-{index}.mp4"));
    let solos = block
        .cues
        .iter()
        .enumerate()
        .filter(|(_, cue)| cue.playback_mode == "play_solo" && cue.media_type == "video")
        .collect::<Vec<_>>();
    let mut command = Command::new("ffmpeg");
    command
        .args(["-y", "-i"])
        .arg(&visual)
        .arg("-i")
        .arg(&block.audio);
    for (_, cue) in &solos {
        command
            .args([
                "-ss",
                &format!("{:.6}", cue.in_point_us as f64 / 1_000_000.0),
                "-i",
            ])
            .arg(&cue.path);
    }
    if solos.is_empty() {
        command.args(["-map", "0:v:0", "-map", "1:a:0"]);
    } else {
        let mut filters = vec!["[1:a]aresample=48000[narr]".to_owned()];
        let mut labels = vec!["[narr]".to_owned()];
        for (solo_index, (cue_index, cue)) in solos.iter().enumerate() {
            let end = block
                .cues
                .get(cue_index + 1)
                .map_or(block.duration_us, |next| next.start_us);
            filters.push(format!(
                "[{}:a]atrim=0:{:.6},asetpts=PTS-STARTPTS,adelay={}|{}[solo{}]",
                solo_index + 2,
                (end - cue.start_us) as f64 / 1_000_000.0,
                cue.start_us / 1_000,
                cue.start_us / 1_000,
                solo_index
            ));
            labels.push(format!("[solo{solo_index}]"));
        }
        filters.push(format!(
            "{}amix=inputs={}:normalize=0:dropout_transition=0[aout]",
            labels.join(""),
            labels.len()
        ));
        command.args([
            "-filter_complex",
            &filters.join(";"),
            "-map",
            "0:v:0",
            "-map",
            "[aout]",
        ]);
    }
    command
        .args([
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-ar",
            "48000",
            "-shortest",
            "-movflags",
            "+faststart",
        ])
        .arg(&output);
    run(&mut command)?;
    Ok(output)
}

fn render_cue(cue: &Cue, duration_us: i64, dimensions: &str, output: &Path) -> Result<()> {
    let duration = format!("{:.6}", duration_us as f64 / 1_000_000.0);
    let seek = format!("{:.6}", cue.in_point_us as f64 / 1_000_000.0);
    let filter = format!("scale={dimensions}:force_original_aspect_ratio=increase,crop={dimensions},setsar=1,fps=30,format=yuv420p");
    let mut command = Command::new("ffmpeg");
    command.arg("-y");
    if cue.media_type == "image" {
        command.args(["-loop", "1", "-i"]);
    } else {
        command.args(["-ss", &seek, "-i"]);
    }
    command
        .arg(&cue.path)
        .args([
            "-t", &duration, "-an", "-vf", &filter, "-c:v", "libx264", "-preset", "veryfast",
            "-crf", "18", "-pix_fmt", "yuv420p",
        ])
        .arg(output);
    run(&mut command)
}

fn concat(files: &[PathBuf], list_path: &Path, output: &Path, audio: bool) -> Result<()> {
    fs::write(
        list_path,
        files
            .iter()
            .map(|path| format!("file '{}'\n", path.to_string_lossy().replace('\'', "'\\''")))
            .collect::<String>(),
    )?;
    let mut command = Command::new("ffmpeg");
    command
        .args(["-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(list_path)
        .args(["-c:v", "copy"]);
    if audio {
        command.args(["-c:a", "aac", "-b:a", "192k"]);
    } else {
        command.arg("-an");
    }
    command.args(["-movflags", "+faststart"]).arg(output);
    run(&mut command)
}
fn run(command: &mut Command) -> Result<()> {
    let output = command.output().context("Failed to start FFmpeg")?;
    if !output.status.success() {
        bail!(
            "FFmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    Ok(())
}
fn ensure_ffmpeg() -> Result<()> {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        bail!("FFmpeg is not installed");
    }
    Ok(())
}
fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}
#[cfg(test)]
mod tests {
    use super::safe_name;
    #[test]
    fn names_are_portable() {
        assert_eq!(safe_name("My Great Video!"), "my-great-video");
    }
}
