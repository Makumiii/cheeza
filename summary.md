# Cheeza engineering handoff

Updated: 2026-07-02 (Africa/Nairobi)  
Repository: `git@github.com:Makumiii/cheeza.git`  
Branch: `main`  
Latest implementation commit: `c620057`
Release preparation base: `a8a0d86`
Current release tag: `v0.1.2`

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
- Persisted production settings for background music, automatic ducking, opening cards, caption presets, and cut/dissolve transitions.
- Full-project 360p preview, final JPEG thumbnail sidecar, and public tagged-release publishing.
- Safe block editor with split-at-cursor and merge-next actions.
- Play-once/loop controls for video and GIF occurrences.
- Guided two-second microphone sound check with quiet/hot/silent feedback.
- Interrupted-take metadata and automatic FFmpeg repair on project reopen.
- Project trash actions for unused media and old takes; source files are moved rather than permanently deleted.
- Alignment confidence and recognized transcript context in take review.

Validation completed locally for the `v0.1.2` source tree:

- `cargo test`: 8 passed, 1 ignored end-to-end test.
- Strict `cargo clippy --all-targets -- -D warnings`: passed.
- `npm run build`: passed.
- `npm run lint`: passed.
- Ignored real FFmpeg fixture test: passed; repairs an interrupted take and verifies dialogue enhancement, opening card, dissolve, ducked music, styled exact-script captions, H.264/AAC 1080x1920 export, thumbnail sidecar, and 360x640 project preview.
- Real faster-whisper synthetic-speech test was run earlier and returned aligned transcript/timestamps.
- Linux `.deb` output: `app/src-tauri/target/release/bundle/deb/Cheeza_0.1.2_amd64.deb`.
- `.deb` metadata and dynamic libraries validated; packaged GUI survived a timed launch under WSL (only expected EGL/Mesa warnings).
- Linux package SHA-256: `fddbfc598a0331fe8b8482631f665b1df34470e554edf654dc835a8564b8266d`.

Remote validation:

- Cross-platform CI for `2089b69`: success, run `28582626479`.
- Windows release for `e0ac745`: success; produced a 580,893,134-byte `Cheeza-Windows` artifact, run `28581473442`.
- Windows release for `44db9dd`: success, run `28582154732`.
- Windows release for `2089b69`: success, run `28582626451`.
- `v0.1.0` Windows release for `c620057`: run `28584671576`.
- `v0.1.1` Windows release for `a8a0d86`: run `28584791777`.
- Final public `v0.1.2` Windows release includes the packaged-dialog capability correction. Run `28585673211` succeeded and published `Cheeza_0.1.2_x64-setup.exe` (580,805,829 bytes).

## Immediate remaining release work

1. Confirm `.github/workflows/windows-smoke.yml` succeeds. It installs the public `v0.1.2` NSIS package, validates bundled tools/model, health-checks the speech worker, and launch-smokes the installed GUI.
2. The public installer size is 580,805,829 bytes. A complete local checksum download was abandoned because the sandbox transfer was throttled; GitHub Actions artifact/release upload integrity and the installed-release smoke test are the authoritative gates.
3. On an actual Windows machine, install the final NSIS artifact and manually test:
   - create/open project;
   - `.txt` import;
   - microphone enumeration and one real recording;
   - pause versus media break timing;
   - trimmed voice-over and play-solo video cues;
   - automatic caption alignment without Python or FFmpeg installed system-wide;
   - 9:16 and 16:9 exports and SRT sidecars;
   - reopening the portable project.
4. Confirm cross-platform CI for `a8a0d86`. Windows releases now run only for manual dispatch or `v*` tags.

## Product gaps beyond the completed core workflow

These were discussed or appeared in the broader plan but are not implemented. Treat them as follow-up scope, not as already working:

- True live voice-following teleprompter. Current live prompter is WPM-paced and holds during media breaks; accurate speech alignment happens after the take.
- Standalone source-audio cues paired with required visuals.
- Undo/restore-from-trash UI and updater/signing infrastructure.
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
