use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command};

pub fn enhance_dialogue(input: &Path, output: &Path) -> Result<()> {
    let result = Command::new("ffmpeg").args(["-y", "-i"]).arg(input).args(["-af", "highpass=f=70,afftdn=nr=10:nf=-45:tn=1,deesser=i=0.25:m=0.45:f=0.55,acompressor=threshold=-18dB:ratio=3:attack=15:release=180:makeup=2,loudnorm=I=-16:LRA=7:TP=-1.5", "-ar", "48000", "-ac", "1", "-c:a", "pcm_s16le"]).arg(output).output().context("Failed to start dialogue enhancement")?;
    if !result.status.success() {
        bail!(
            "Dialogue enhancement failed: {}",
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
