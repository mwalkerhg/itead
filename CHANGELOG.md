# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed
- Migrated from Rust CLI to Tauri 2 desktop app with SvelteKit frontend
- Audio engine extracted into `engine.rs` module with `EngineHandle` / `AudioEngine` separation
- Recording is now on-demand (start/stop via UI) instead of a CLI flag at launch
- Replaced `clap` CLI args with GUI controls for device selection, sample rate, buffer size, and mono mode
- Replaced `ctrlc` shutdown handler with Tauri-managed lifecycle

### Added
- SvelteKit frontend with device selection, transport controls, and status display
- Tauri commands: `list_audio_devices`, `start_engine`, `start_recording`, `stop_engine`
- Thread-safe `EngineHandle` with command channel pattern for controlling the engine from the UI thread

## [0.1.0] — 2026-06-10

### Added
- Initial Rust CLI audio engine
- Real-time audio passthrough using cpal and lock-free ring buffers (rtrb)
- Input/output device selection via `--input` / `--output` substring match
- `--list-devices` to enumerate available audio devices with supported configs
- Configurable sample rate (`--sample-rate`) and buffer size (`--buffer-size`)
- Mono mode (`--mono`) — takes channel 1 and duplicates to both L+R
- WAV recording (`--record`) with optional output path (`--output-file`)
- Graceful shutdown via Ctrl+C with final buffer drain
- MIT license
