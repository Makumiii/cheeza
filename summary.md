# Cheeza engineering handoff

Updated: 2026-07-02 (Africa/Nairobi)  
Repository: `git@github.com:Makumiii/cheeza.git`  
Branch: `main`  
Latest implementation commit: `2089b69`
Handoff/workflow cleanup commit: `82e98b0`

## Product

Cheeza is a local-first, Windows-first desktop studio for making script-led informational videos. The creator imports or pastes a script, records narration in paragraph-sized blocks while presenting images/videos from a cue tray, and exports a mastered, captioned MP4. Presentation is reconstructed from native cue events; the editor itself is never screen-recorded.

The core UX principles agreed with the user are:

- Script blocks are the source of truth for structure and exact caption text.
- Record in focused blocks, then assemble automatically.
- Media lives in a project-wide dock and is arranged into a per-block presentation tray.
- During recording, previous/next/direct cue activation must be immediate.
- `Pause` freezes recording time. `Media break` keeps project time moving, writes silence to narration, and allows a video/GIF to play alone.
- Tray video occurrences have independent trim ranges and either `Voice over` or `Play solo` behavior.
- Retakes are non-destructive; one accepted take per block drives captions and export.
- Default outputs target mobile consumption: 9:16, with 16:9 also supported.
- Processing is offline and project data stays local.

## Stack and layout

- `app/`: React 19 + TypeScript + Vite frontend.
- `app/src-tauri/`: Tauri 2 shell and Rust core.
- `app/src-tauri/src/project.rs`: portable folder projects, SQLite schema/migrations, scripts, assets, trays, takes.
- `recorder.rs`: native CPAL mono WAV capture, pause/media-break state, cue timestamps, input level.
- `media.rs`: FFprobe metadata plus thumbnail and 720p editing proxy creation.
- `audio.rs`: FFmpeg dialogue enhancement.
- `speech.rs`: offline worker invocation and exact-script word timestamp persistence.
- `captions.rs`: aligned/fallback SRT generation.
- `render.rs`: event-driven FFmpeg composition and final H.264/AAC MP4 export.
- `tools.rs`: bundled FFmpeg/FFprobe/speech-worker discovery with PATH fallback.
- `workers/speech_worker.py`: faster-whisper `small.en`, CPU int8 transcription, exact-script alignment.
- `.github/workflows/ci.yml`: Ubuntu/Windows build, test, format, and strict Clippy.
- `.github/workflows/windows-release.yml`: bundles FFmpeg, FFprobe, PyInstaller speech worker, offline model, and builds NSIS.

Projects are portable folders containing `cheeza.sqlite`, originals, proxies, thumbnails, raw/processed takes, captions, cache, exports, and trash directories.

## Implemented and verified

- Create/open projects, recent-project shortcuts, 9:16 and 16:9 targets.
- Paste or import `.txt` scripts; paragraph splitting, safe reconciliation after edits, block reordering.
- Media import for JPG/JPEG/PNG/WebP/GIF, MP4/MOV/M4V/WebM, WAV/MP3/M4A/AAC/FLAC/OGG/Opus.
- Content-hash deduplication, copied originals, metadata, thumbnails, proxies.
- Per-block visual cue trays, cue reordering/removal, occurrence trim in/out, voice-over/play-solo mode.
- Audio-only files are accepted into the project dock but rejected as visual cues.
- Microphone selection, live input meter, countdown, pause/resume, media breaks, direct/previous/next cues, stop-and-save.
- Non-destructive retakes with accepted-take selection and in-editor playback.
- Raw recording preservation and processed take creation. If enhancement fails, raw audio is copied forward rather than losing the take.
- Automatic post-take offline alignment of exact script words. Manual re-alignment remains available.
- Event-driven render from original media, trim-aware video playback, play-solo audio mixing, anti-click block fades.
- Exact-script SRT sidecar and burned captions. Vertical caption safe margin differs from landscape.
- H.264 video, AAC 48 kHz audio, `faststart`, 1080x1920 or 1920x1080 MP4.
- Custom Cheeza desktop icon and dark/orange desktop UI.
- Bundled Windows offline toolchain; packaged speech-worker health check runs before installer build.

Validation completed locally on `2089b69`:

- `cargo test`: 7 passed, 1 ignored end-to-end test.
- Strict `cargo clippy --all-targets -- -D warnings`: passed.
- `npm run build`: passed.
- `npm run lint`: passed.
- Ignored real FFmpeg fixture test: passed; verifies H.264, AAC, 1080x1920, and exact-script SRT.
- Real faster-whisper synthetic-speech test was run earlier and returned aligned transcript/timestamps.
- Linux `.deb` built at `app/src-tauri/target/release/bundle/deb/Cheeza_0.1.0_amd64.deb`.
- `.deb` metadata and dynamic libraries validated; packaged GUI survived a timed launch under WSL (only expected EGL/Mesa warnings).
- Linux package SHA-256: `53c7e43fb0a3a1e84e56a5c3cdcf9624eb51cec9063af14928dc351f719ba69a`.

Remote validation:

- Cross-platform CI for `2089b69`: success, run `28582626479`.
- Windows release for `e0ac745`: success; produced a 580,893,134-byte `Cheeza-Windows` artifact, run `28581473442`.
- Windows release for `44db9dd`: success, run `28582154732`.
- Final Windows release for `2089b69`: run `28582626451`; it was still in progress when this document was written. Monitor until success and confirm `Cheeza-Windows` artifact exists.

## Immediate remaining release work

1. Wait for Windows release run `28582626451` to finish and confirm success/artifact.
2. Inspect artifact metadata/size. GitHub's artifact ZIP endpoint requires authenticated access; the public Actions page still exposes run status.
3. On an actual Windows machine, install the final NSIS artifact and manually test:
   - create/open project;
   - `.txt` import;
   - microphone enumeration and one real recording;
   - pause versus media break timing;
   - trimmed voice-over and play-solo video cues;
   - automatic caption alignment without Python or FFmpeg installed system-wide;
   - 9:16 and 16:9 exports and SRT sidecars;
   - reopening the portable project.
4. Confirm normal CI for the handoff/workflow-cleanup commit. The temporary Windows `main` push trigger has already been removed; `workflow_dispatch` and `v*` tag triggers remain.

## Product gaps beyond the completed core workflow

These were discussed or appeared in the broader plan but are not implemented. Treat them as follow-up scope, not as already working:

- Project-wide background music, automatic ducking, and music controls.
- Opening thumbnail/title card and thumbnail sidecar export.
- Named caption style/brand presets and editable caption mismatch UI.
- True live voice-following teleprompter. Current live prompter is WPM-paced and holds during media breaks; accurate speech alignment happens after the take.
- Full-project low-resolution preview. The top-right action performs the production export.
- Visual dissolves or configurable transitions. Current visuals use deterministic cuts; narration blocks receive short anti-click fades.
- Standalone source-audio cues paired with required visuals.
- Dedicated GIF play-once/loop controls.
- Split/merge buttons for individual blocks. Users currently adjust paragraph boundaries in the source-script editor.
- Trash/undo UI, explicit crash-recovery UI for orphan raw files, and updater/signing infrastructure.
- An automated Windows GUI test harness. Core logic and packaging are automated; actual microphone/WebView interaction still needs Windows manual QA.

Do not silently claim these follow-up items are complete. Discuss prioritization with the user before expanding scope.

## Commands

From repository root:

```sh
npm ci --prefix app
npm run build --prefix app
npm run lint --prefix app
cargo fmt --check --manifest-path app/src-tauri/Cargo.toml
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo clippy --all-targets --manifest-path app/src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml renders_fixture_project_end_to_end -- --ignored --nocapture
npm run tauri --prefix app -- build --bundles deb
```

Development caption alignment expects `.venv/bin/python` or `CHEEZA_PYTHON`, plus `workers/requirements.txt`. Development rendering expects FFmpeg/FFprobe on PATH. Windows release bundles all of these.

## Working rules and credentials

- Preserve user changes and use `apply_patch` for source edits.
- The worktree was clean immediately before this handoff file was added.
- The Git SSH key prompts for a passphrase. The user supplied it in conversation; never store it in files, commits, shell history, logs, or documentation. Enter credentials only through an interactive prompt.
- Do not commit generated models, FFmpeg binaries, `.venv`, build output, project media, or secrets.
- The user explicitly requested persistence until the app is done and tested. Report concrete validation evidence and be candid about follow-up scope.
