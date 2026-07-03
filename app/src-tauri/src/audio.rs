use anyhow::{bail, Context, Result};
use std::path::Path;

const OUTPUT_ARGS: &[&str] = &["-ar", "48000", "-ac", "1", "-c:a", "pcm_s16le"];

pub fn enhance_dialogue(input: &Path, output: &Path) -> Result<()> {
    let filters = [
        "highpass=f=80,afftdn=nr=10:nf=-45:tn=1,acompressor=threshold=-18dB:ratio=3:attack=15:release=180:makeup=2,loudnorm=I=-16:LRA=7:TP=-1.5",
        "highpass=f=80,acompressor=threshold=-18dB:ratio=3:attack=15:release=180:makeup=2,loudnorm=I=-16:LRA=7:TP=-1.5",
        "aresample=48000,aformat=sample_fmts=s16:channel_layouts=mono",
    ];

    let mut last_error = String::new();
    for filter in filters {
        match run_filter(input, output, filter) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error.to_string(),
        }
    }

    bail!("Dialogue enhancement failed: {last_error}")
}

fn run_filter(input: &Path, output: &Path, filter: &str) -> Result<()> {
    let mut command = crate::tools::command("ffmpeg");
    command
        .args(["-y", "-i"])
        .arg(input)
        .args(["-af", filter])
        .args(OUTPUT_ARGS)
        .arg(output);

    let result = command
        .output()
        .context("Failed to start dialogue enhancement")?;
    if !result.status.success() {
        bail!(
            "{}",
            String::from_utf8_lossy(&result.stderr)
                .lines()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    Ok(())
}
