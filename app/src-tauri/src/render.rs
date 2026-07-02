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
    pub caption_path: String,
    pub thumbnail_path: String,
}
struct ExportSettings {
    music_path: Option<PathBuf>,
    music_volume: f64,
    music_ducking: bool,
    opening_card: bool,
    opening_title: String,
    caption_style: String,
    transition_style: String,
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
    out_point_us: Option<i64>,
    playback_mode: String,
    loop_mode: String,
    source_duration_us: Option<i64>,
}

pub fn export(project_path: &str) -> Result<ExportResult> {
    render_project(project_path, false)
}

pub fn preview(project_path: &str) -> Result<ExportResult> {
    render_project(project_path, true)
}

fn render_project(project_path: &str, preview: bool) -> Result<ExportResult> {
    ensure_ffmpeg()?;
    let root = PathBuf::from(project_path);
    let db = Connection::open(root.join("cheeza.sqlite"))?;
    let (name, aspect, music_asset_id, music_volume, music_ducking, opening_card, opening_title, caption_style, transition_style): (String, String, Option<String>, f64, i64, i64, String, String, String) = db.query_row(
        "SELECT name,aspect_ratio,background_music_asset_id,music_volume,music_ducking,opening_card,opening_title,caption_style,transition_style FROM project LIMIT 1",
        [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
    )?;
    let music_path = music_asset_id
        .map(|id| {
            db.query_row(
                "SELECT relative_path FROM media_assets WHERE id=?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
        })
        .transpose()?
        .map(|path| root.join(path));
    let settings = ExportSettings {
        music_path,
        music_volume,
        music_ducking: music_ducking != 0,
        opening_card: opening_card != 0,
        opening_title: if opening_title.trim().is_empty() {
            name.clone()
        } else {
            opening_title
        },
        caption_style,
        transition_style,
    };
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
        if preview {
            "360:640"
        } else {
            "1080:1920"
        }
    } else {
        if preview {
            "640:360"
        } else {
            "1920:1080"
        }
    };
    let mut rendered = Vec::new();
    if settings.opening_card {
        rendered.push((
            render_opening(&settings.opening_title, &work, dimensions)?,
            1_500_000,
        ));
    }
    for (index, block) in blocks.iter().enumerate() {
        rendered.push((
            render_block(block, &work, index, dimensions)?,
            block.duration_us,
        ));
    }
    let assembled = work.join("assembled.mp4");
    assemble(
        &rendered,
        &settings.transition_style,
        &work.join("blocks.txt"),
        &assembled,
    )?;
    let transition_us = if settings.transition_style == "dissolve" {
        200_000
    } else {
        0
    };
    let opening_offset = if settings.opening_card {
        1_500_000 - transition_us
    } else {
        0
    };
    let caption_path = root.join("captions/captions.srt");
    let mut offset_us = opening_offset;
    let captions = blocks
        .iter()
        .map(|block| {
            let item = crate::captions::CaptionBlock {
                text: &block.text,
                offset_us,
                duration_us: block.duration_us,
            };
            offset_us += block.duration_us - transition_us;
            item
        })
        .collect::<Vec<_>>();
    crate::captions::write_srt(&caption_path, &captions)?;
    if blocks.iter().all(|block| !block.words.is_empty()) {
        let mut aligned_offset = opening_offset;
        let aligned = blocks
            .iter()
            .map(|block| {
                let item = crate::captions::AlignedBlock {
                    offset_us: aligned_offset,
                    words: block.words.clone(),
                };
                aligned_offset += block.duration_us - transition_us;
                item
            })
            .collect::<Vec<_>>();
        crate::captions::write_aligned_srt(&caption_path, &aligned)?;
    }
    let export_stem = format!(
        "{}-{aspect}-{}",
        safe_name(&name),
        Utc::now().format("%Y%m%d-%H%M%S")
    );
    let (output, thumbnail) = if preview {
        (
            root.join("cache/project-preview.mp4"),
            root.join("cache/project-preview.jpg"),
        )
    } else {
        (
            root.join("exports").join(format!("{export_stem}.mp4")),
            root.join("exports")
                .join(format!("{export_stem}-thumbnail.jpg")),
        )
    };
    let subtitle = caption_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    let (mut caption_size, mut caption_margin, outline, back_colour) =
        caption_preset(&settings.caption_style, &aspect);
    if preview {
        caption_size = (caption_size / 3).max(10);
        caption_margin = (caption_margin / 3).max(20);
    }
    let duration_us = rendered.iter().map(|(_, duration)| *duration).sum::<i64>()
        - transition_us * rendered.len().saturating_sub(1) as i64;
    let audio_base = if let Some(music) = &settings.music_path {
        let mixed = work.join("music-mixed.mp4");
        mix_music(
            &assembled,
            music,
            settings.music_volume,
            settings.music_ducking,
            duration_us,
            &mixed,
        )?;
        mixed
    } else {
        assembled
    };
    run(crate::tools::command("ffmpeg").args(["-y", "-i"]).arg(&audio_base).args(["-vf", &format!("subtitles='{subtitle}':force_style='FontName=Arial,FontSize={caption_size},PrimaryColour=&H00FFFFFF,OutlineColour=&H00101010,Outline={outline},BackColour={back_colour},BorderStyle=3,Shadow=0,Alignment=2,MarginV={caption_margin}'"), "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-c:a", "copy", "-movflags", "+faststart"]).arg(&output))?;
    run(crate::tools::command("ffmpeg")
        .args([
            "-y",
            "-ss",
            if settings.opening_card {
                "0.75"
            } else {
                "0.20"
            },
            "-i",
        ])
        .arg(&output)
        .args(["-frames:v", "1", "-q:v", "2"])
        .arg(&thumbnail))?;
    Ok(ExportResult {
        path: output.to_string_lossy().into_owned(),
        duration_us,
        caption_path: caption_path.to_string_lossy().into_owned(),
        thumbnail_path: thumbnail.to_string_lossy().into_owned(),
    })
}

fn caption_preset(style: &str, aspect: &str) -> (i32, i32, i32, &'static str) {
    let margin = if aspect == "9:16" { 260 } else { 70 };
    match style {
        "bold" => (
            if aspect == "9:16" { 40 } else { 34 },
            margin,
            4,
            "&H78000000",
        ),
        "minimal" => (
            if aspect == "9:16" { 30 } else { 25 },
            margin,
            2,
            "&H00000000",
        ),
        _ => (
            if aspect == "9:16" { 34 } else { 28 },
            margin,
            3,
            "&H58000000",
        ),
    }
}

fn render_opening(title: &str, work: &Path, dimensions: &str) -> Result<PathBuf> {
    let subtitle_path = work.join("opening.srt");
    fs::write(
        &subtitle_path,
        format!(
            "1\n00:00:00,000 --> 00:00:01,500\n{}\n",
            title.replace('\n', " ")
        ),
    )?;
    let subtitle = subtitle_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    let output = work.join("opening.mp4");
    let size = dimensions.replace(':', "x");
    let title_size = if dimensions.starts_with("360") || dimensions.starts_with("640") {
        18
    } else {
        48
    };
    let title_margin = if title_size == 18 { 36 } else { 120 };
    run(crate::tools::command("ffmpeg").args(["-y", "-f", "lavfi", "-i", &format!("color=c=0x0d0c0a:s={size}:r=30"), "-f", "lavfi", "-i", "anullsrc=r=48000:cl=mono", "-t", "1.5", "-vf", &format!("subtitles='{subtitle}':force_style='FontName=Arial,FontSize={title_size},Bold=1,PrimaryColour=&H00FFFFFF,OutlineColour=&H00FF7417,Outline=2,Alignment=5,MarginL={title_margin},MarginR={title_margin}'"), "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-shortest", "-movflags", "+faststart"]).arg(&output))?;
    Ok(output)
}

fn assemble(
    clips: &[(PathBuf, i64)],
    transition_style: &str,
    list_path: &Path,
    output: &Path,
) -> Result<()> {
    let paths = clips
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    if transition_style != "dissolve" || clips.len() < 2 {
        return concat(&paths, list_path, output, true);
    }
    const TRANSITION_SECONDS: f64 = 0.2;
    let mut command = crate::tools::command("ffmpeg");
    command.arg("-y");
    for (path, _) in clips {
        command.args(["-i"]).arg(path);
    }
    let mut filters = Vec::new();
    for index in 0..clips.len() {
        filters.push(format!(
            "[{index}:v]settb=AVTB,setpts=PTS-STARTPTS[v{index}]"
        ));
        filters.push(format!(
            "[{index}:a]aresample=48000,asetpts=PTS-STARTPTS[a{index}]"
        ));
    }
    let mut video_label = "[v0]".to_owned();
    let mut audio_label = "[a0]".to_owned();
    let mut current_duration = clips[0].1 as f64 / 1_000_000.0;
    for (index, (_, duration_us)) in clips.iter().enumerate().skip(1) {
        let video_out = format!("[vx{index}]");
        let audio_out = format!("[ax{index}]");
        let offset = (current_duration - TRANSITION_SECONDS).max(0.0);
        filters.push(format!(
            "{video_label}[v{index}]xfade=transition=fade:duration={TRANSITION_SECONDS}:offset={offset:.6}{video_out}"
        ));
        filters.push(format!(
            "{audio_label}[a{index}]acrossfade=d={TRANSITION_SECONDS}:c1=tri:c2=tri{audio_out}"
        ));
        video_label = video_out;
        audio_label = audio_out;
        current_duration += *duration_us as f64 / 1_000_000.0 - TRANSITION_SECONDS;
    }
    command
        .args([
            "-filter_complex",
            &filters.join(";"),
            "-map",
            &video_label,
            "-map",
            &audio_label,
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-ar",
            "48000",
            "-movflags",
            "+faststart",
        ])
        .arg(output);
    run(&mut command)
}

fn mix_music(
    video: &Path,
    music: &Path,
    volume: f64,
    ducking: bool,
    duration_us: i64,
    output: &Path,
) -> Result<()> {
    let music_filter = if ducking {
        format!("[1:a]volume={volume:.3}[music];[music][0:a]sidechaincompress=threshold=0.025:ratio=8:attack=20:release=500[ducked];[0:a][ducked]amix=inputs=2:duration=first:normalize=0[aout]")
    } else {
        format!("[1:a]volume={volume:.3}[music];[0:a][music]amix=inputs=2:duration=first:normalize=0[aout]")
    };
    run(crate::tools::command("ffmpeg")
        .args(["-y", "-i"])
        .arg(video)
        .args(["-stream_loop", "-1", "-i"])
        .arg(music)
        .args([
            "-filter_complex",
            &music_filter,
            "-map",
            "0:v:0",
            "-map",
            "[aout]",
            "-t",
            &format!("{:.6}", duration_us as f64 / 1_000_000.0),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-ar",
            "48000",
            "-movflags",
            "+faststart",
        ])
        .arg(output))
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
        let mut cues_query = db.prepare("SELECT a.relative_path,a.media_type,e.project_time_us,ti.in_point_us,ti.out_point_us,ti.playback_mode,ti.loop_mode,a.duration_us FROM presentation_events e JOIN tray_items ti ON ti.id=e.tray_item_id JOIN media_assets a ON a.id=ti.asset_id WHERE e.take_id=?1 AND e.event_type='activate' ORDER BY e.project_time_us")?;
        let cues = cues_query
            .query_map(params![take_id], |row| {
                Ok(Cue {
                    path: root.join(row.get::<_, String>(0)?),
                    media_type: row.get(1)?,
                    start_us: row.get(2)?,
                    in_point_us: row.get(3)?,
                    out_point_us: row.get(4)?,
                    playback_mode: row.get(5)?,
                    loop_mode: row.get(6)?,
                    source_duration_us: row.get(7)?,
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
    let mut command = crate::tools::command("ffmpeg");
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
        let fade_out = (block.duration_us as f64 / 1_000_000.0 - 0.03).max(0.0);
        command.args([
            "-filter_complex",
            &format!("[1:a]afade=t=in:st=0:d=0.03,afade=t=out:st={fade_out:.6}:d=0.03[aout]"),
            "-map",
            "0:v:0",
            "-map",
            "[aout]",
        ]);
    } else {
        let mut filters = vec!["[1:a]aresample=48000[narr]".to_owned()];
        let mut labels = vec!["[narr]".to_owned()];
        for (solo_index, (cue_index, cue)) in solos.iter().enumerate() {
            let end = block
                .cues
                .get(cue_index + 1)
                .map_or(block.duration_us, |next| next.start_us);
            let source_duration = cue
                .out_point_us
                .map(|out| (out - cue.in_point_us).max(40_000))
                .unwrap_or(end - cue.start_us)
                .min(end - cue.start_us);
            filters.push(format!(
                "[{}:a]atrim=0:{:.6},asetpts=PTS-STARTPTS,adelay={}|{}[solo{}]",
                solo_index + 2,
                source_duration as f64 / 1_000_000.0,
                cue.start_us / 1_000,
                cue.start_us / 1_000,
                solo_index
            ));
            labels.push(format!("[solo{solo_index}]"));
        }
        filters.push(format!(
            "{}amix=inputs={}:normalize=0:dropout_transition=0[mixed]",
            labels.join(""),
            labels.len()
        ));
        let fade_out = (block.duration_us as f64 / 1_000_000.0 - 0.03).max(0.0);
        filters.push(format!(
            "[mixed]afade=t=in:st=0:d=0.03,afade=t=out:st={fade_out:.6}:d=0.03[aout]"
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
    let base_filter = format!("scale={dimensions}:force_original_aspect_ratio=increase,crop={dimensions},setsar=1,fps=30,format=yuv420p");
    let animated = cue.media_type == "video"
        || cue
            .path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("gif"));
    let filter = if animated {
        let available = cue
            .out_point_us
            .map(|out| (out - cue.in_point_us).max(40_000))
            .or_else(|| {
                cue.source_duration_us
                    .map(|source| (source - cue.in_point_us).max(40_000))
            })
            .unwrap_or(duration_us)
            .min(duration_us);
        if cue.loop_mode == "repeat" {
            let frames = ((available as f64 / 1_000_000.0) * 30.0).ceil().max(1.0) as i64;
            format!("trim=duration={:.6},setpts=PTS-STARTPTS,{base_filter},loop=loop=-1:size={frames}:start=0,setpts=N/FRAME_RATE/TB", available as f64 / 1_000_000.0)
        } else {
            format!("trim=duration={:.6},setpts=PTS-STARTPTS,{base_filter},tpad=stop_mode=clone:stop_duration={:.6}", available as f64 / 1_000_000.0, duration_us as f64 / 1_000_000.0)
        }
    } else {
        base_filter
    };
    let mut command = crate::tools::command("ffmpeg");
    command.arg("-y");
    if cue.media_type == "image" && !animated {
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
    let mut command = crate::tools::command("ffmpeg");
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
    if crate::tools::command("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
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
    use super::{export, preview, safe_name};
    use crate::{
        models::{CreateProjectInput, UpdateProjectSettingsInput},
        project,
    };
    use std::process::Command;
    #[test]
    fn names_are_portable() {
        assert_eq!(safe_name("My Great Video!"), "my-great-video");
    }

    #[test]
    #[ignore = "requires FFmpeg and performs a full 1080p encode"]
    fn renders_fixture_project_end_to_end() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.png");
        let music = temp.path().join("music.wav");
        assert!(Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=0xEE6C1A:s=640x360",
                "-frames:v",
                "1"
            ])
            .arg(&source)
            .output()
            .unwrap()
            .status
            .success());
        assert!(Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=110:duration=4",
                "-ar",
                "48000",
                "-ac",
                "2",
            ])
            .arg(&music)
            .output()
            .unwrap()
            .status
            .success());
        let snapshot = project::create(CreateProjectInput {
            parent_path: temp.path().to_string_lossy().into_owned(),
            name: "Fixture".into(),
            aspect_ratio: "9:16".into(),
            platform_target: "TikTok".into(),
        })
        .unwrap();
        let snapshot = project::save_script(
            &snapshot.path,
            "Every great story begins with a clear point of view.",
        )
        .unwrap();
        let snapshot = project::import_media(
            &snapshot.path,
            &[
                source.to_string_lossy().into_owned(),
                music.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        let image_id = snapshot
            .assets
            .iter()
            .find(|asset| asset.media_type == "image")
            .unwrap()
            .id
            .clone();
        let music_id = snapshot
            .assets
            .iter()
            .find(|asset| asset.media_type == "audio")
            .unwrap()
            .id
            .clone();
        let snapshot =
            project::add_tray_item(&snapshot.path, &snapshot.blocks[0].id, &image_id).unwrap();
        let snapshot = project::update_settings(
            &snapshot.path,
            UpdateProjectSettingsInput {
                background_music_asset_id: Some(music_id),
                music_volume: 0.12,
                music_ducking: true,
                opening_card: true,
                opening_title: "Fixture Story".into(),
                caption_style: "bold".into(),
                transition_style: "dissolve".into(),
            },
        )
        .unwrap();
        let root = std::path::PathBuf::from(&snapshot.path);
        let audio = root.join("recordings/raw/interrupted.wav");
        assert!(Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=220:duration=2",
                "-ar",
                "48000",
                "-ac",
                "1"
            ])
            .arg(&audio)
            .output()
            .unwrap()
            .status
            .success());
        std::fs::write(
            root.join("cache/active-recording.json"),
            serde_json::to_vec(&crate::recorder::RecoveryMetadata {
                take_id: "take".into(),
                block_id: snapshot.blocks[0].id.clone(),
                relative_path: "recordings/raw/interrupted.wav".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
            })
            .unwrap(),
        )
        .unwrap();
        let snapshot = project::open(&snapshot.path).unwrap();
        assert_eq!(snapshot.blocks[0].takes.len(), 1);
        assert!(!root.join("cache/active-recording.json").exists());
        let result = export(&snapshot.path).unwrap();
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,width,height",
                "-of",
                "compact",
            ])
            .arg(&result.path)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&probe.stdout);
        assert!(probe.status.success());
        assert!(
            report.contains("codec_name=h264")
                && report.contains("codec_name=aac")
                && report.contains("width=1080")
                && report.contains("height=1920")
        );
        let captions = std::fs::read_to_string(root.join("captions/captions.srt")).unwrap();
        assert!(captions.contains("Every great story begins"));
        assert!(std::path::Path::new(&result.thumbnail_path).is_file());
        assert!(std::path::Path::new(&result.caption_path).is_file());
        assert!(result.duration_us > 3_000_000);
        let preview_result = preview(&snapshot.path).unwrap();
        let preview_probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=width,height",
                "-of",
                "compact",
            ])
            .arg(&preview_result.path)
            .output()
            .unwrap();
        let preview_report = String::from_utf8_lossy(&preview_probe.stdout);
        assert!(preview_report.contains("width=360") && preview_report.contains("height=640"));
    }
}
