# Floe

Floe is a minimal desktop push-to-talk transcription utility. Hold a global hotkey, speak, release it, and Floe sends the completed recording once to Groq Speech-to-Text, cleans the transcript locally, copies it to the clipboard, and pastes it into the focused app.

This repository is currently an early V1 implementation. The desktop shell, settings storage, manual recording, Groq transcription, clipboard writes, and paste automation are in place; global push-to-talk hotkeys are intentionally not implemented yet.

## Product Goal

Floe aims to feel fast, private by default, and boringly reliable. V1 intentionally favors a single complete transcription request after recording stops over streaming or partial transcript features.

## Tech Stack

- Tauri 2
- React, TypeScript, Vite
- Rust backend
- Future `cpal` microphone recording
- Future in-memory 16-bit PCM WAV generation
- Future Groq Speech-to-Text with `whisper-large-v3-turbo`
- OS keychain storage for Groq API keys

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
- React status screen, secure settings controls, manual recording controls, and transcript copy/paste actions.
- Rust commands for app status, secure settings, recording checks, Groq transcription, clipboard writes, and paste automation.
- No global hotkeys yet.
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

## Manual Test Flow

1. Run `pnpm tauri:dev`.
2. Save a Groq API key in the settings panel.
3. Click `Start`, speak briefly, then click `Stop`.
4. Focus a target text field in another app.
5. Return to Floe and click `Transcribe + paste`.
6. Confirm the cleaned transcript appears in Floe and is pasted into the focused target.
7. If the OS blocks paste automation, Floe keeps the transcript on the clipboard. Paste manually with Command+V on macOS or Control+V on Windows/Linux.

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

Groq API keys are stored through the operating system keychain using the Rust `keyring` crate. Non-secret app settings are stored separately in Floe's app config directory.

The frontend never receives the full Groq API key. It only receives whether a key is configured and a masked preview such as `gsk_...abcd`.

If the native keychain is unavailable in the current environment, Floe does not fall back to plaintext secret files. Saving or clearing a secret returns a sanitized error, and the API key status remains unconfigured until OS keychain access works.

## Troubleshooting

- If `pnpm` is not on PATH, run commands through Corepack: `corepack pnpm ...`.
- If `tauri:dev` fails on Linux, install the WebKitGTK and appindicator packages listed in `.github/workflows/ci.yml`.
- Runtime transcription features are placeholders until their dedicated implementation tasks.

## Testing and CI

Tests must not call the real Groq API, require a real key, or depend on a real microphone. Use mocks and fakes for API and audio pipeline tests.

GitHub Actions runs frontend formatting, linting, tests, builds, Rust formatting, Rust linting, Rust tests, and basic secret scanning support.

## Security Notes

Never commit secrets or temporary audio files. `.env` files are ignored and should only be used for local development metadata, not production Groq keys.

Enable GitHub secret scanning and push protection in the repository security settings when those features are available for the repository plan. If GitHub secret scanning is unavailable for a private repository, run a local scan before pushing:

```powershell
gitleaks detect --source . --redact --no-git
```
