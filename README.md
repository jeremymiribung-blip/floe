# Floe

Floe is a minimal desktop push-to-talk transcription utility. Hold a global hotkey, speak, release it, and Floe sends the completed recording once to Groq Speech-to-Text, cleans the transcript with Groq, copies the cleaned text to the clipboard, and pastes it into the focused app.

This repository is currently an early V1 implementation. The desktop shell, settings storage, manual recording, configurable global push-to-talk hotkeys, Groq transcription, Groq transcript cleanup, clipboard writes, paste automation, and a minimal first-run onboarding flow are in place.

## Product Goal

Floe aims to feel fast, private by default, and boringly reliable. The STT path intentionally favors a single complete transcription request after recording stops over streaming or partial transcript features. Transcript cleanup is fixed and uses Groq; there is no user-selectable provider or cleanup mode.

## Tech Stack

- Tauri 2
- React, TypeScript, Vite
- Rust backend
- `cpal` microphone recording
- In-memory 16-bit PCM WAV generation
- Groq Speech-to-Text with `whisper-large-v3-turbo`
- Groq transcript cleanup with `openai/gpt-oss-20b`
- OS keychain storage for the Groq API key
- Tauri autostart integration for optional start-at-login

## Intended V1 Scope

1. Start recording on push-to-talk.
2. Stop recording on release.
3. Convert the full recording to WAV in memory.
4. Send the WAV once to Groq STT.
5. Clean the transcript with Groq.
6. Copy and paste the final text.

Retries are bounded and only used for temporary network/API failures. If Groq cleanup fails, Floe falls back to the raw Groq transcript and surfaces a `Cleanup failed` warning.

## Non-goals

Floe does not include streaming, rolling transcription, audio chunking, overlap windows, realtime partial transcripts, transcript merging, analytics, or permanent audio storage. Floe also does not expose a user-selectable cleanup provider or mode; the only flow is Groq STT followed by Groq cleanup using the same API key.

## Current Scaffold Scope

- Minimal Tauri 2 app named Floe.
- React UI with a first-run onboarding flow (Groq API key, then hotkey), a minimal overview (wordmark, current state, current hotkey, `Settings`, `Diagnostics`), and a settings view (`API Key`, `Hotkey`, `Start at login`, `Privacy`).
- Rust commands for app status, secure settings, recording checks, Groq transcription, Groq cleanup, clipboard writes, and paste automation.
- A small local `Diagnostics` action that copies the latest pipeline timing JSON.
- Tauri 2 global shortcut registration with press/release events for push-to-talk.
- Optional start-at-login support that launches Floe hidden in the background.
- GitHub Actions CI for frontend and Rust checks.

## First Run

The first time Floe is opened, the main window shows a short onboarding flow instead of the overview:

1. **Groq API key** — enter the Groq API key and press `Continue`. The key is stored in the OS keychain and the app moves to the next step.
2. **Hotkey** — confirm the default hotkey (`Ctrl + Space` on Windows/Linux, `Option + Space` on macOS) or press `Change` to capture a new shortcut, then press `Continue`.
3. **Overview** — the minimal overview appears, showing the wordmark, status, hotkey, `Settings`, and `Diagnostics`.

Floe only calls Groq for transcription and cleanup; it does not validate the API key with a network call during onboarding. If the key is cleared later or the hotkey becomes invalid, Floe returns to the matching onboarding step. The main overview never shows cleanup modes, provider labels, or behavior settings. Floe is Groq-only.

Floe is Groq-only and intentionally minimal. There are no cleanup modes, no Behavior section, and no provider switching — the only flow is `Groq STT → Groq cleanup → clipboard → paste`.

## Privacy Model

- Audio is kept in memory and sent once to Groq after recording stops.
- Audio is not written to disk by default.
- Audio is never sent for cleanup. Only transcript text is sent to Groq for cleanup.
- If Groq cleanup fails, Floe pastes the raw Groq transcript and shows a short `Cleanup failed` warning.
- The Groq API key is stored locally in the OS keychain.
- Enabling Start at login does not access the microphone, start recording, call Groq, paste text, or send transcript data on startup.
- Debug logging avoids raw audio, raw transcripts, full API keys, auth headers, and private transcripts.
- Local diagnostics are kept in memory only and are never sent anywhere. They include timing and outcome metadata, but not transcripts, cleaned text, raw audio, API keys, auth headers, raw Groq responses, or clipboard contents.

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
2. On the first launch, the Groq API key step appears. Enter a Groq API key and press `Continue`.
3. The Hotkey step appears. Press `Continue` to keep the default, or `Change` to capture a new shortcut, then `Continue`.
4. The minimal overview appears showing the wordmark, status, current hotkey, and a `Settings` link.
5. Focus a target text field in another app.
6. Hold the configured global hotkey, speak briefly, then release it.
7. Confirm the cleaned transcript is pasted into the focused target.
8. If the OS blocks paste automation, Floe shows `Copied` on the status view. Paste manually with Command+V on macOS or Control+V on Windows/Linux.
9. Use `Change` in the Hotkey section, press a new key combination, and confirm Floe re-registers it. `Reset` restores the platform default.
10. Enable `Start at login`, quit Floe from the tray, log out/in or restart, and confirm Floe starts hidden with the tray icon present and the hotkey active.
11. In Settings, clear the Groq API key. The app returns to the Groq setup step automatically. Re-enter the key and continue to the Hotkey step.

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

- macOS: `Alt+Space` shown as `Option + Space`.
- Windows/Linux: `Control+Space` shown as `Ctrl + Space`.

Change the hotkey from Settings with `Change hotkey`, then press the new shortcut. Press `Escape` or `Cancel` to leave the current shortcut unchanged. Floe validates the shortcut, registers it with the OS, saves it in non-secret app settings, and restores the previous working shortcut if the new one cannot be registered. `Reset default` restores the platform default.

Hotkey settings are stored separately from API keys. API keys remain in the OS keychain; the hotkey is the only non-secret app setting.

## Start at Login

Start at login is optional. When enabled from Settings, Floe registers with the operating system to launch after user login with a background startup argument. Floe starts hidden, creates the tray icon, initializes app state, and registers the configured global hotkey so push-to-talk is available immediately.

Use the tray `Show Floe` menu item to open the main window after a background start. Use tray `Quit` to fully exit Floe. Disabling Start at login removes the OS autostart registration.

Autostart behavior may depend on OS-specific login item permissions. On Linux, availability can vary by desktop environment and tray/AppIndicator support.

## Tray and Window Lifecycle

Floe is a background push-to-talk utility. The window close button (X on Windows/Linux, red close button on macOS, and `Cmd+W`) hides Floe to the system tray instead of quitting. The global hotkey keeps working while the window is hidden, and the tray icon stays active. The tray menu offers `Show Floe`, `Hide Floe`, `Settings`, and `Quit`. Use the tray `Quit` to fully exit Floe; `Cmd+Q` on macOS also exits through the same shutdown path. On Linux desktops without a system tray, Floe falls back to keeping the process running but the tray icon may not be visible.

## Single Instance

Floe runs as a single app instance. Launching Floe again from the Start Menu, Desktop, or Explorer shows and focuses the existing main window instead of starting a second process. This prevents duplicate tray icons, duplicate global hotkey registrations, and double recording or paste attempts. When Start at login is enabled, autostart and a later manual launch resolve to one running instance; the secondary launch shows the existing window. After tray `Quit`, the next launch starts normally.

## API Keys and Cleanup

The Groq API key is stored through the operating system keychain using the Rust `keyring` crate under the `groq-api-key` user. Non-secret app settings, including the global hotkey, are stored separately in Floe's app config directory.

The frontend never receives the full API key. It only receives whether a key is configured and a masked preview such as `gsk_...abcd`.

If the native keychain is unavailable in the current environment, Floe does not fall back to plaintext secret files. Saving or clearing a secret returns a sanitized error, and the API key status remains unconfigured until OS keychain access works.

Cleanup is fixed: after Groq STT, Floe sends the transcript text to Groq using the same API key. Only transcript text is sent for cleanup; audio is never sent. If Groq cleanup fails (for example, missing or invalid key, network error, rate limit, malformed response), Floe pastes the raw Groq transcript and surfaces a short `Cleanup failed` warning. The flow does not block on cleanup success.

## Troubleshooting

- If `pnpm` is not on PATH, run commands through Corepack: `corepack pnpm ...`.
- If `tauri:dev` fails on Linux, install the WebKitGTK and appindicator packages listed in `.github/workflows/ci.yml`.
- If Groq cleanup is slow or unavailable, Floe will fall back to the raw Groq transcript and show `Cleanup failed`. The rest of the flow still works.
- If the hotkey does not register, choose a less common shortcut; another app or the OS may already own it.
- On macOS, allow Floe in Privacy & Security settings if global shortcuts or paste automation are blocked. Depending on the OS version, Accessibility and Input Monitoring permissions may be relevant.
- On Windows/Linux, desktop environments and input methods can reserve shortcuts. Try `Control+Shift+KeyB` or another two-modifier combination if registration fails.
- If Start at login is unavailable, check OS login item permissions and desktop environment support. Linux tray visibility depends on the desktop shell and AppIndicator support.
- If clicking the window X seems to make Floe disappear, look in the system tray. Floe stays alive in the tray so the global hotkey keeps working. Use the tray `Quit` menu to fully exit. On Linux desktops without an AppIndicator extension, the tray icon may not be visible; use the OS task manager to quit if needed.

## Testing and CI

Tests must not call the real Groq API, require real keys, or depend on a real microphone. Use mocks and fakes for API and audio pipeline tests.

GitHub Actions runs frontend formatting, linting, tests, builds, Rust formatting, Rust linting, Rust tests, and basic secret scanning support.

## Security Notes

Never commit secrets or temporary audio files. `.env` files are ignored and should only be used for local development metadata, not production Groq keys.

Enable GitHub secret scanning and push protection in the repository security settings when those features are available for the repository plan. If GitHub secret scanning is unavailable for a private repository, run a local scan before pushing:

```powershell
gitleaks detect --source . --redact --no-git
```
