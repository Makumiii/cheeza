use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command};

#[derive(Debug, Deserialize)]
struct WorkerResult {
    transcript: String,
    words: Vec<AlignedWord>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AlignedWord {
    pub word: String,
    pub start_us: i64,
    pub end_us: i64,
    pub matched: bool,
}

pub fn align_block(project_path: &str, block_id: &str) -> Result<Vec<AlignedWord>> {
    let root = PathBuf::from(project_path);
    let mut db = Connection::open(root.join("cheeza.sqlite"))?;
    let (script, audio, take_id): (String,String,String) = db.query_row("SELECT b.text,COALESCE(t.processed_relative_path,t.relative_path),t.id FROM script_blocks b JOIN takes t ON t.block_id=b.id AND t.selected=1 WHERE b.id=?1", params![block_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).context("The block needs an accepted recording before alignment")?;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository = manifest
        .parent()
        .and_then(|path| path.parent())
        .context("Cannot locate Cheeza worker")?;
    let python = std::env::var_os("CHEEZA_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository.join(".venv/bin/python"));
    let worker = repository.join("workers/speech_worker.py");
    let output = Command::new(&python)
        .arg(&worker)
        .args(["--audio"])
        .arg(root.join(audio))
        .args(["--script", &script, "--model", "small.en"])
        .output()
        .with_context(|| format!("Could not start speech worker with {}", python.display()))?;
    if !output.status.success() {
        bail!(
            "Speech alignment failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let result: WorkerResult =
        serde_json::from_slice(&output.stdout).context("Speech worker returned invalid JSON")?;
    let transaction = db.transaction()?;
    transaction.execute(
        "DELETE FROM aligned_words WHERE take_id=?1",
        params![take_id],
    )?;
    for (position, word) in result.words.iter().enumerate() {
        transaction.execute("INSERT INTO aligned_words(take_id,position,word,start_us,end_us,matched) VALUES(?1,?2,?3,?4,?5,?6)", params![take_id,position as i64,word.word,word.start_us,word.end_us,word.matched])?;
    }
    transaction.execute(
        "UPDATE script_blocks SET alignment_stale=0 WHERE id=?1",
        params![block_id],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO transcripts(take_id,text) VALUES(?1,?2)",
        params![take_id, result.transcript],
    )?;
    transaction.commit()?;
    Ok(result.words)
}
