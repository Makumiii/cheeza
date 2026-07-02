"""Offline transcription and script alignment worker for Cheeza."""
from __future__ import annotations

import argparse
import json
import re
from difflib import SequenceMatcher
from pathlib import Path


def normalize(word: str) -> str:
    return re.sub(r"[^a-z0-9']", "", word.lower())


def align_script(script: str, recognized: list[dict]) -> list[dict]:
    script_words = script.split()
    source_words = [item["word"].strip() for item in recognized]
    matcher = SequenceMatcher(None, [normalize(word) for word in script_words], [normalize(word) for word in source_words], autojunk=False)
    timings: list[tuple[float, float] | None] = [None] * len(script_words)
    for block in matcher.get_matching_blocks():
        for offset in range(block.size):
            source = recognized[block.b + offset]
            timings[block.a + offset] = (float(source["start"]), float(source["end"]))

    default_step = max((recognized[-1]["end"] / max(len(script_words), 1)) if recognized else 0.35, 0.08)
    for index, timing in enumerate(timings):
        if timing is not None:
            continue
        previous = next((timings[pos] for pos in range(index - 1, -1, -1) if timings[pos]), None)
        following = next((timings[pos] for pos in range(index + 1, len(timings)) if timings[pos]), None)
        start = previous[1] if previous else max(0.0, (following[0] - default_step) if following else index * default_step)
        end = min(following[0], start + default_step) if following else start + default_step
        timings[index] = (start, max(start + 0.04, end))

    return [{"word": word, "start_us": round(timings[index][0] * 1_000_000), "end_us": round(timings[index][1] * 1_000_000), "matched": normalize(word) in {normalize(item) for item in source_words}} for index, word in enumerate(script_words)]


def transcribe(audio: Path, script: str, model_name: str) -> dict:
    from faster_whisper import WhisperModel

    model = WhisperModel(model_name, device="cpu", compute_type="int8", cpu_threads=2)
    segments, info = model.transcribe(str(audio), language="en", beam_size=5, vad_filter=True, word_timestamps=True, initial_prompt=script)
    words = [{"word": word.word, "start": word.start, "end": word.end, "probability": word.probability} for segment in segments for word in (segment.words or [])]
    return {"language": info.language, "confidence": info.language_probability, "transcript": "".join(word["word"] for word in words).strip(), "words": align_script(script, words)}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--health", action="store_true")
    parser.add_argument("--audio", type=Path)
    parser.add_argument("--script")
    parser.add_argument("--model", default="small.en")
    args = parser.parse_args()
    if args.health:
        print(json.dumps({"ok": True}))
        return
    if not args.audio or args.script is None:
        parser.error("--audio and --script are required")
    print(json.dumps(transcribe(args.audio, args.script, args.model)))


if __name__ == "__main__":
    main()
