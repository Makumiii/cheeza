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

## Creator workflow

1. Create a vertical or landscape project and paste a script or import a `.txt` file. Paragraphs become recording blocks.
2. Import project media. Add images, GIFs, or video clips to each block's presentation tray, then set trim, play-once/loop, and voice-over/play-solo behavior.
3. Select and test the microphone. Record the block while advancing cues with the tray, arrow keys, or direct clicks. Pause freezes time; Media break lets source media play without narration.
4. Stop to preserve and master the take. Cheeza aligns the exact script for captions automatically; retakes remain available for review.
5. Use Production settings for an opener, background music/ducking, captions, and transitions. Preview the assembled low-resolution cut.
6. Export the final MP4. Cheeza writes a burned-caption H.264/AAC video, an exact-script SRT, and a JPEG thumbnail into the project `exports` folder.

Interrupted recordings are repaired when the project is reopened. Deleting media or takes moves their files into the project `trash` folder rather than permanently erasing them.

## Releases

Tagged Windows builds include FFmpeg, FFprobe, the offline speech worker, and the `small.en` model; no separate media or Python installation is required. Release installers are published on the repository's GitHub Releases page. Windows installers are currently unsigned, so Windows may show its standard publisher warning.

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
