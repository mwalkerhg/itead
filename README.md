# ITEAD

A desktop audio engine built with [Tauri 2](https://v2.tauri.app/), [SvelteKit](https://svelte.dev/), and Rust. Routes audio from an input device to an output device in real time with optional WAV recording.

## Features

- Real-time audio passthrough (input → output)
- Device selection (input and output)
- Configurable sample rate (44100 / 48000 / 96000 Hz)
- Configurable buffer size (64–1024 frames)
- Mono mode (channel 1 → both L+R)
- WAV recording (32-bit float)

## Architecture

```
┌──────────────────────────────────┐
│  SvelteKit Frontend(+page.svelte)│
│  Device selection, transport UI  │
└──────────┬───────────────────────┘
           │ invoke()
┌──────────▼───────────────────────┐
│  Tauri Commands(lib.rs)          │
│  State management, IPC bridge    │
└──────────┬───────────────────────┘
           │
┌──────────▼───────────────────────┐
│  Audio Engine(engine.rs)         │
│  cpal streams, ring buffers,     │
│  WAV writer thread               │
└──────────────────────────────────┘
```

- **Frontend** — Svelte 5 with `@tauri-apps/api` for calling Rust commands
- **Tauri layer** — manages `EngineHandle` behind a `Mutex`, exposes commands to the frontend
- **Engine** — runs audio I/O on a dedicated thread, uses lock-free ring buffers (`rtrb`) for real-time safety

## Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

## Getting Started

```bash
# Install frontend dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | SvelteKit, TypeScript, Svelte 5 |
| Desktop framework | Tauri 2 |
| Audio I/O | cpal 0.15 |
| Ring buffer | rtrb 0.3 (lock-free, real-time safe) |
| WAV writing | hound 3.5 |
| Error handling | anyhow |
| Serialization | serde |

## License

[MIT](LICENSE)
