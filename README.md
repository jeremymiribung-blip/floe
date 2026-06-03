# Floe

Floe is a minimal desktop push-to-talk transcription utility. Hold a global hotkey, speak, release it, and Floe sends the completed recording once to Groq Speech-to-Text, cleans the transcript according to the selected cleanup mode, copies it to the clipboard, and pastes it into the focused app.

This repository is currently an early V1 implementation. The desktop shell, settings storage, manual recording, configurable global push-to-talk hotkeys, Groq transcription, clipboard writes, and paste automation are in place.

## Product Goal

Floe aims to feel fast, private by default, and boringly reliable. The STT path intentionally favors a single complete transcription request after recording stops over streaming or partial transcript features.

## Tech Stack

- Tauri 2
- React, TypeScript, Vite
- Rust backend
- `cpal` microphone recording
- In-memory 16-bit PCM WAV generation
- Groq Speech-to-Text with `whisper-large-v3-turbo`
- Optional Cerebras cleanup with `gpt-oss-120b`
- OS keychain storage for Groq and Cerebras API keys

## Intended V1 Scope

1. Start recording on push-to-talk.
2. Stop recording on release.
3. Convert the full recording to WAV in memory.
4. Send the WAV once to Groq STT.
5. Clean the transcript with the selected cleanup mode.
6. Copy and paste the final text.

Retries are bounded and only used for temporary network/API failures.

## Non-goals

Floe does not include streaming, rolling transcription, audio chunking, overlap windows, realtime partial transcripts, transcript merging, analytics, or permanent audio storage.

## Current Scaffold Scope

- Minimal Tauri 2 app named Floe.
- React UI with a status view (wordmark, current state, current hotkey) and a settings view (`API Keys`, `Hotkey`, `Privacy`).
- Rust commands for app status, secure settings, recording checks, Groq transcription, transcript cleanup, clipboard writes, and paste automation.
- Tauri 2 global shortcut registration with press/release events for push-to-talk.
- GitHub Actions CI for frontend and Rust checks.

## Privacy Model

- Audio is kept in memory and sent once to Groq after recording stops.
- Audio is not written to disk by default.
- Audio is never sent to Cerebras.
- Transcript text is only sent to Cerebras when the user explicitly enables Clean cleanup.
- Raw mode pastes the Groq transcript unchanged.
- Fast mode uses local cleanup and remains the default.
- Clean mode is optional, disabled by default, and may send transcript text to Cerebras.
- API keys are stored locally in the OS keychain and kept separate by provider.
- Debug logging avoids raw audio, raw transcripts, full API keys, auth headers, and private transcripts.

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
2. Save a Groq API key in Settings, then save a Cerebras API key if you want to test Cerebras cleanup.
3. Confirm the global hotkey appears as the current shortcut on the status view.
4. Focus a target text field in another app.
5. Hold the configured global hotkey, speak briefly, then release it.
6. Confirm the cleaned transcript is pasted into the focused target.
7. If the OS blocks paste automation, Floe shows `Copied` on the status view. Paste manually with Command+V on macOS or Control+V on Windows/Linux.
8. Use `Change` in the Hotkey section, press a new key combination, and confirm Floe re-registers it. `Reset` restores the platform default.

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

## Hotkey Settings

Floe uses the Tauri 2 global shortcut plugin and listens for both press and release events. The default push-to-talk hotkey is:

- macOS: `CommandOrControl+Shift+Space` shown as `Command+Shift+Space`.
- Windows/Linux: `Control+Shift+Space`.

Change the hotkey from Settings with `Change hotkey`, then press the new shortcut. Press `Escape` or `Cancel` to leave the current shortcut unchanged. Floe validates the shortcut, registers it with the OS, saves it in non-secret app settings, and restores the previous working shortcut if the new one cannot be registered. `Reset default` restores the platform default.

Hotkey settings are stored separately from API keys. API keys remain in the OS keychain; the hotkey and cleanup mode are non-secret app settings.

## Tray and Window Lifecycle

Floe is a background push-to-talk utility. The window close button (X on Windows/Linux, red close button on macOS, and `Cmd+W`) hides Floe to the system tray instead of quitting. The global hotkey keeps working while the window is hidden, and the tray icon stays active. The tray menu offers `Show Floe`, `Hide Floe`, `Settings`, and `Quit`. Use the tray `Quit` to fully exit Floe; `Cmd+Q` on macOS also exits through the same shutdown path. On Linux desktops without a system tray, Floe falls back to keeping the process running but the tray icon may not be visible.

## API Keys and Cleanup Settings

Groq and Cerebras API keys are stored through the operating system keychain using the Rust `keyring` crate. Each provider uses a separate keychain entry. Non-secret app settings, including cleanup mode and global hotkey, are stored separately in Floe's app config directory.

The frontend never receives a full API key. It only receives whether a key is configured and a masked preview such as `gsk_...abcd` or `csk_...abcd`.

If the native keychain is unavailable in the current environment, Floe does not fall back to plaintext secret files. Saving or clearing a secret returns a sanitized error, and the API key status remains unconfigured until OS keychain access works.

Cleanup modes:

- `Raw`: paste the raw Groq transcript with no cleanup.
- `Fast`: use local cleanup. This is the default.
- `Clean`: use Cerebras AI cleanup after Groq STT. Only transcript text is sent to Cerebras; audio is never sent.

The UI does not expose the cleanup mode. If `Clean` is active and the Cerebras key is missing or the Cerebras request fails, Floe pastes Fast-cleaned text and the status view surfaces a short error or warning.

## Troubleshooting

- If `pnpm` is not on PATH, run commands through Corepack: `corepack pnpm ...`.
- If `tauri:dev` fails on Linux, install the WebKitGTK and appindicator packages listed in `.github/workflows/ci.yml`.
- If `Clean` is slower than expected, switch back to `Fast`; Clean depends on Cerebras availability, latency, rate limits, and key validity.
- If the hotkey does not register, choose a less common shortcut; another app or the OS may already own it.
- On macOS, allow Floe in Privacy & Security settings if global shortcuts or paste automation are blocked. Depending on the OS version, Accessibility and Input Monitoring permissions may be relevant.
- On Windows/Linux, desktop environments and input methods can reserve shortcuts. Try `Control+Alt+Shift+Space` or another three-key combination if registration fails.
- If clicking the window X seems to make Floe disappear, look in the system tray. Floe stays alive in the tray so the global hotkey keeps working. Use the tray `Quit` menu to fully exit. On Linux desktops without an AppIndicator extension, the tray icon may not be visible; use the OS task manager to quit if needed.

## Testing and CI

Tests must not call the real Groq or Cerebras APIs, require real keys, or depend on a real microphone. Use mocks and fakes for API and audio pipeline tests.

GitHub Actions runs frontend formatting, linting, tests, builds, Rust formatting, Rust linting, Rust tests, and basic secret scanning support.

## Security Notes

Never commit secrets or temporary audio files. `.env` files are ignored and should only be used for local development metadata, not production Groq or Cerebras keys.

Enable GitHub secret scanning and push protection in the repository security settings when those features are available for the repository plan. If GitHub secret scanning is unavailable for a private repository, run a local scan before pushing:

```powershell
gitleaks detect --source . --redact --no-git
```
