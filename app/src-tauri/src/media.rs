use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
pub struct PreparedMedia {
    pub duration_us: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub proxy_relative_path: Option<String>,
    pub thumbnail_relative_path: Option<String>,
}

#[derive(Deserialize)]
struct Probe {
    streams: Vec<Stream>,
    format: Format,
}
#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    duration: Option<String>,
}
#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
}

pub fn prepare(
    root: &Path,
    asset_id: &str,
    source_relative: &str,
    media_type: &str,
) -> Result<PreparedMedia> {
    let source = root.join(source_relative);
    let probe = probe(&source)?;
    let video = probe
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"));
    let duration = video
        .and_then(|stream| stream.duration.as_deref())
        .or(probe.format.duration.as_deref())
        .and_then(|value| value.parse::<f64>().ok())
        .map(|seconds| (seconds * 1_000_000.0) as i64);
    let thumbnail_relative = format!("assets/thumbnails/{asset_id}.jpg");
    let thumbnail = root.join(&thumbnail_relative);
    if media_type != "audio" {
        run(Command::new("ffmpeg")
            .args(["-y", "-ss", "0", "-i"])
            .arg(&source)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=480:-2:force_original_aspect_ratio=decrease",
                "-q:v",
                "3",
            ])
            .arg(&thumbnail))?;
    }
    let proxy_relative = if media_type == "video" {
        let relative = format!("assets/proxies/{asset_id}.mp4");
        run(Command::new("ffmpeg")
            .args(["-y", "-i"])
            .arg(&source)
            .args([
                "-map",
                "0:v:0",
                "-an",
                "-vf",
                "scale=720:1280:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "25",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(root.join(&relative)))?;
        Some(relative)
    } else {
        None
    };
    Ok(PreparedMedia {
        duration_us: duration,
        width: video.and_then(|stream| stream.width),
        height: video.and_then(|stream| stream.height),
        proxy_relative_path: proxy_relative,
        thumbnail_relative_path: (media_type != "audio").then_some(thumbnail_relative),
    })
}

fn probe(path: &Path) -> Result<Probe> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("Failed to start ffprobe")?;
    if !output.status.success() {
        bail!(
            "Could not inspect media: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn run(command: &mut Command) -> Result<()> {
    let output = command.output().context("Failed to start FFmpeg")?;
    if !output.status.success() {
        bail!(
            "Media preparation failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    Ok(())
}

#[allow(dead_code)]
fn _portable(path: PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}
