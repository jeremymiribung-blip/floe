# Changelog

All notable changes to Floe will be documented in this file.

## [Unreleased]

### Added

- Single-instance enforcement via the Tauri 2 single-instance plugin. Secondary launches show and focus the existing main window instead of reinitializing the tray, global hotkey, audio manager, recording, Groq calls, or paste. This makes Start at login plus a later manual launch resolve to a single running instance.

- Groq transcript cleanup as part of the fixed Groq STT → Groq cleanup → clipboard → paste flow. The `cleanup_transcript` command now always sends the Groq transcript through the Groq Chat Completions API and copies the result to the clipboard; if cleanup fails, the raw Groq transcript is pasted instead and a short `Cleanup failed` warning is shown.
- Best-effort migration of any legacy `cerebras-api-key` keychain entry to silence in `SettingsManager::new`. The legacy value is never read or logged.
- Initial setup-only Tauri 2, React, TypeScript, Vite, and Rust scaffold.
- Minimal Floe UI with a status indicator, settings placeholder, and manual-test placeholder buttons.
- Stub-only Tauri commands for app status, settings, and manual-test checks.
- Clipboard write and paste automation commands for the manual transcription flow.
- Single Groq API key storage and masked key status in settings.
- Reliable configurable global push-to-talk hotkey registration with startup fallback and settings controls.
- Optional Start at login setting that launches Floe hidden in the background, creates the tray icon, and registers the global hotkey after user login.
- Minimal recording bubble: a frameless, always-on-top overlay with a volume-reactive audio bar visualization, shown only while recording.
- Repository docs, issue templates, PR template, MIT license, `.env.example`, `.gitignore`, and GitHub Actions CI.

### Removed

- The Cerebras transcript cleanup provider and all related code (Rust `cerebras` module, frontend wrappers, settings, commands, and tests).
- The Cerebras-only API key row, masked Cerebras key status, and Cerebras keyring entry.
- The `CleanupMode` enum and its `Raw`, `Fast`, and `Clean` variants (introduced in the prior release and now fully retired).
- The `cleanupMode` field on `AppSettings` (Rust and TypeScript).
- The `get_cleanup_mode` and `set_cleanup_mode` Tauri commands and their browser fallbacks.
- The `Fast` fallback that re-cleaned text locally when Cerebras cleanup failed; the cleanup path now falls back to the raw Groq transcript instead.
- The `cleanup_transcript_local` helper and the `src/lib/transcriptCleanup.ts` local cleanup module.
- The `SettingsErrorCode::MissingCerebrasApiKey` and `SettingsErrorCode::InvalidCerebrasApiKey` variants and the `"missingCerebrasApiKey"` / `"invalidCerebrasApiKey"` `SettingsError` codes, since the new cleanup path never hard-errors on a missing key.
- Tests that asserted mode persistence, `mode: "raw" | "fast" | "clean"` return values, force-fast fallback on Cerebras key removal, or Cerebras-only key storage and masking.

### Changed

- Refactored transcript cleanup to use Groq Chat Completions with `openai/gpt-oss-20b` and a strict system prompt; cleanup is a single non-streaming call, with bounded retries for network/timeout/429/5xx and respect for `Retry-After`. Only transcript text is sent; audio is never sent for cleanup.
- The main window close button now hides Floe to the system tray instead of quitting the app, so the global push-to-talk hotkey remains registered and active while the window is hidden. Use the new tray `Quit` menu item to fully exit Floe. The tray menu now offers `Show Floe`, `Hide Floe`, `Settings`, and `Quit`. Closing the window while recording continues to record and does not interrupt the audio stream; closing while quitting first unregisters the hotkey and stops recording safely through the existing shutdown path.
- Simplified the desktop UI to a minimal, light, black-and-white, two-view design: a status view showing only the wordmark, current state, current hotkey, and a `Settings` link, plus a settings view with `API Keys`, `Hotkey`, `Start at login`, and `Privacy` sections.
- `AppState` now includes `ready` and `copied` so the status view can show `Ready` and `Copied` instead of internal "Idle" and "Needs attention" labels.
- Privacy note in Settings now reads `Audio → Groq` and `Text → Groq`, reflecting the single-provider flow.
