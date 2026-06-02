# Floe

Floe is a minimal desktop push-to-talk transcription utility. Hold a global hotkey, speak, release it, and Floe sends the completed recording once to Groq Speech-to-Text, cleans the transcript locally, copies it to the clipboard, and pastes it into the focused app.

This repository is currently at the setup-only scaffold milestone. The desktop shell, project structure, CI, and stub commands are in place; audio capture, Groq calls, hotkeys, clipboard writes, and paste automation are intentionally not implemented yet.

## Product Goal

Floe aims to feel fast, private by default, and boringly reliable. V1 intentionally favors a single complete transcription request after recording stops over streaming or partial transcript features.

## Tech Stack

- Tauri 2
- React, TypeScript, Vite
- Rust backend
- Future `cpal` microphone recording
- Future in-memory 16-bit PCM WAV generation
- Future Groq Speech-to-Text with `whisper-large-v3-turbo`
- Future OS keychain storage

## Intended V1 Scope

1. Start recording on push-to-talk.
2. Stop recording on release.
3. Convert the full recording to WAV in memory.
4. Send the WAV once to Groq STT.
5. Clean the transcript locally.
6. Copy and paste the final text.

Retries are bounded and only used for temporary network/API failures.

## Non-goals

V1 does not include streaming, rolling transcription, audio chunking, overlap windows, realtime partial transcripts, transcript merging, LLM cleanup, analytics, or permanent audio storage.

## Current Scaffold Scope

- Minimal Tauri 2 app named Floe.
- React status screen, settings placeholder, and manual testing placeholder buttons.
- Rust stub commands only.
- No microphone access, network calls, secret storage, clipboard writes, hotkeys, or paste automation.
- GitHub Actions CI for frontend and Rust checks.

## Privacy Model

- Audio is kept in memory and sent once to Groq after recording stops.
- Audio is not written to disk by default.
- Transcript text is only used for the paste flow.
- No transcript text is sent to an LLM for cleanup in V1.
- API keys are stored locally in the OS keychain.
- Debug logging avoids raw audio, full API keys, auth headers, and private transcripts.

## Setup

Install Node.js, Rust stable, and pnpm. If pnpm is not installed, enable it through Corepack:

```powershell
corepack enable
corepack prepare pnpm@10.12.1 --activate
```

Then install dependencies:

```powershell
pnpm install
```

## Development

```powershell
pnpm dev
pnpm tauri:dev
```

Useful checks:

```powershell
pnpm format
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## Groq API Key

Groq API key storage is not implemented in the scaffold. When added, production keys should be stored through the OS keychain and the frontend should only receive whether a key exists.

## Troubleshooting

- If `pnpm` is not on PATH, run commands through Corepack: `corepack pnpm ...`.
- If `tauri:dev` fails on Linux, install the WebKitGTK and appindicator packages listed in `.github/workflows/ci.yml`.
- Runtime transcription features are placeholders until their dedicated implementation tasks.

## Testing and CI

Tests must not call the real Groq API, require a real key, or depend on a real microphone. Use mocks and fakes for API and audio pipeline tests.

GitHub Actions runs frontend formatting, linting, tests, builds, Rust formatting, Rust linting, Rust tests, and basic secret scanning support.

## Security Notes

Never commit secrets or temporary audio files. `.env` files are ignored and should only be used for local development metadata, not production Groq keys.
