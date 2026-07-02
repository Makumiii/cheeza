# Cheeza

Cheeza is a local-first desktop studio for producing script-driven informational videos. It combines block-based narration, a voice-following teleprompter, live visual cueing, automatic captions, dialogue enhancement, and native-quality export.

## Current development state

The production implementation is under active development. The first vertical slice includes portable project folders, versioned SQLite persistence, script blocks, copied media assets, and per-block presentation trays.

## Development

Prerequisites:

- Node.js 24+
- Rust stable
- Tauri 2 system dependencies

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

## License

Cheeza is licensed under the GNU Affero General Public License v3.0. See `LICENSE`.
