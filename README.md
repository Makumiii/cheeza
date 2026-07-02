# Cheeza

Cheeza is a local-first desktop studio for producing script-driven informational videos. It combines block-based narration, a paced teleprompter, live visual cueing, exact-script captions, dialogue enhancement, and native-quality export.

## What it does

- Imports pasted or `.txt` scripts and turns paragraphs into reorderable recording blocks.
- Copies supported image, GIF, video, and audio assets into a portable project folder and creates editing proxies.
- Records non-destructive narration takes with pause, media-break, previous/next cue, microphone selection, and a live input meter.
- Captures presentation events natively; it does not screen-record the editor.
- Masters dialogue and aligns the exact script to offline speech timestamps automatically.
- Exports captioned H.264/AAC MP4 in vertical 9:16 or landscape 16:9, plus an SRT sidecar.
- Runs locally. Project media and recordings are not uploaded.

## Development

Prerequisites:

- Node.js 24+
- Rust stable
- Tauri 2 system dependencies
- FFmpeg and FFprobe on `PATH` (development builds)
- Python 3.12 with `workers/requirements.txt` for local caption alignment

```sh
cd app
npm install
npm run tauri dev
```

Validation:

```sh
cd app
npm run build
cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
```

The Windows release workflow produces an NSIS installer with FFmpeg, FFprobe, the offline speech worker, and the `small.en` model bundled. Tagged releases use tags matching `v*`.

## License

Cheeza is licensed under the GNU Affero General Public License v3.0. See `LICENSE`.
