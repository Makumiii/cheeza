use anyhow::Result;
use std::{fs, path::Path};
pub struct CaptionBlock<'a> {
    pub text: &'a str,
    pub offset_us: i64,
    pub duration_us: i64,
}
#[derive(Clone)]
pub struct TimedWord {
    pub word: String,
    pub start_us: i64,
    pub end_us: i64,
}
pub struct AlignedBlock {
    pub offset_us: i64,
    pub words: Vec<TimedWord>,
}

pub fn write_aligned_srt(path: &Path, blocks: &[AlignedBlock]) -> Result<()> {
    let mut index = 1;
    let mut output = String::new();
    for block in blocks {
        for words in block.words.chunks(6) {
            if words.is_empty() {
                continue;
            }
            let start = block.offset_us + words[0].start_us;
            let end = block.offset_us + words.last().expect("non-empty chunk").end_us;
            let phrase = words
                .iter()
                .map(|word| word.word.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            output.push_str(&format!(
                "{index}\n{} --> {}\n{phrase}\n\n",
                timestamp(start),
                timestamp(end)
            ));
            index += 1;
        }
    }
    fs::write(path, output)?;
    Ok(())
}
pub fn write_srt(path: &Path, blocks: &[CaptionBlock<'_>]) -> Result<()> {
    let mut index = 1;
    let mut output = String::new();
    for block in blocks {
        let phrases = phrase_chunks(block.text, 6);
        if phrases.is_empty() {
            continue;
        }
        let share = block.duration_us / phrases.len() as i64;
        for (position, phrase) in phrases.iter().enumerate() {
            let start = block.offset_us + share * position as i64;
            let end = if position + 1 == phrases.len() {
                block.offset_us + block.duration_us
            } else {
                start + share
            };
            output.push_str(&format!(
                "{index}\n{} --> {}\n{phrase}\n\n",
                timestamp(start),
                timestamp(end)
            ));
            index += 1;
        }
    }
    fs::write(path, output)?;
    Ok(())
}
fn phrase_chunks(text: &str, maximum: usize) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut current = Vec::new();
    for word in text.split_whitespace() {
        current.push(word);
        if current.len() >= maximum || word.ends_with(['.', '!', '?', ';']) {
            phrases.push(current.join(" "));
            current.clear();
        }
    }
    if !current.is_empty() {
        phrases.push(current.join(" "));
    }
    phrases
}
fn timestamp(us: i64) -> String {
    let ms = us.max(0) / 1_000;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}
#[cfg(test)]
mod tests {
    use super::{phrase_chunks, timestamp};
    #[test]
    fn grouping() {
        assert_eq!(
            phrase_chunks("One two three. Four five six seven", 3),
            ["One two three.", "Four five six", "seven"]
        )
    }
    #[test]
    fn time() {
        assert_eq!(timestamp(3_723_456_000), "01:02:03,456")
    }
}
