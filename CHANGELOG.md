# Changelog

All notable changes to Floe will be documented in this file.## [1.3.0] - 2026-07-07

### Note

- Both v1.1.1 and v1.2.0 are intentionally bypassed — neither was tagged or published. The queued fixes (cleanup-model migration, keychain error UX, watchdog timing fix, diagnostics-log redaction, usePushToTalk refactor) ship together here as v1.3.0. The migration lands now because Groq is deprecating the previously planned `llama-3.3-70b-versatile` cleanup model on 2026-08-16 and the rest of the Llama Production family is being retired on the same date, so Floe moves to `qwen/qwen3.6-27b` (Preview-tier) before that deadline rather than ship a release that would 400 on cleanup calls the day it installs. One coordinated update for users instead of two.
- The Windows installer (`Floe_1.3.0_x64-setup.exe` / `.msi`) is **unsigned** in this release. The GitHub Actions release pipeline does not yet have `WINDOWS_CERTIFICATE` or `TAURI_SIGNING_PRIVATE_KEY` secrets configured, so Microsoft SmartScreen may surface a warning on first run. Linux `.deb` / `.AppImage` / `.rpm` and macOS `.dmg` packages are unaffected.
- The new cleanup model `qwen/qwen3.6-27b` is currently **Preview-tier** on Groq. Preview models can be retired on short notice. Track Groq's deprecation page before any future release that touches the cleanup pipeline; if Groq ships this model as Production, keep it; if Groq retires it, migrate to the next Production-tier chat model.

### Changed

- Groq transcript cleanup model is now `qwen/qwen3.6-27b` (Preview-tier). Floe still talks to the same Groq Chat Completions endpoint with the same keychain-stored API key; there is no provider switching, no cleanup modes, no behaviour toggles, no Cerebras. AGENTS.md is updated to lift the prior "no Qwen cleanup" rule; the "no GPT-OSS cleanup" rule remains in force.
- Cleanup HTTP request body: the Qwen-inference `chat_template_kwargs` payload is intentionally still **not** sent. The strict `validate_cleanup_output` validator in `src-tauri/src/providers/groq/cleanup.rs` already rejects Markdown / JSON / YAML / commentary wrappers and would catch stray Qwen thinking tags, triggering the documented `Cleanup failed` + raw-paste fallback. If the Qwen model on Groq starts returning thinking tokens in normal output, the validator gets a tighter heuristic before Floe re-introduces model-specific request parameters; AGENTS.md would be updated accordingly.
- `usePushToTalk` no longer reads `skipCleanup` from the store on each render. Instead, the controller mounts one Zustand `subscribe` inside a `useEffect` and mirrors the latest `skipCleanup` into a ref, so the controller's long-lived `cleanupTranscript` closure reacts to in-app "skip cleanup" toggles across the controller's full lifetime.
- Settings window now awaits the in-flight API-key save before hiding, so a save failure isn't silently dropped when the user closes mid-save.
- Onboarding stores the trimmed API-key value in the global store on every keystroke, so settings never pre-fill an input with leading or trailing whitespace from this draft.

### Added

- User-facing differentiation for keychain storage failures in onboarding and settings. A failing `saveApiKey` rejection with `code === "secretStoreUnavailable"` now surfaces a dedicated message ("Could not save your API key: your system's keychain is unavailable. Check that your OS keychain is unlocked …") instead of the previous generic network-error copy. New helpers in `src/lib/errorLog.ts`: `isKeychainError(err)` recognises the structured Rust `SettingsError` code, and `KEYCHAIN_UNAVAILABLE_MESSAGE` carries the user-facing string. Both the `GroqStep` (onboarding) and `SettingsWindow` (key-validation rejection) branches independently branch on the helper so users with a locked or inaccessible OS keychain are not sent chasing a network problem that does not exist.
- Diagnostics-log redaction surfaced across the IPC boundary. The `diag_log_str` Tauri command now scrubs every incoming frontend line through `diag::report::redact_string_for_report` before writing to the diagnostics log. Accidentally-logged bearer tokens, API keys, transcripts, clipboard snippets, or other secret-shaped content from the frontend land as `"redacted"` on disk instead of the raw value. `redact_string_for_report` has been promoted from `#[cfg(test)]` to a public function on `crate::diag::report` so production callers no longer need the test-only carve-out.

### Fixed

- Recording watchdog overshoot. The watchdog loop in `src-tauri/src/recording/mod.rs` now records its start time as a `std::time::Instant` and recomputes the sleep cap each iteration as `timeout.saturating_sub(start.elapsed()).min(poll_interval)`. The previous `elapsed += sleep_duration` accumulator could drift over a long single sleep and silently overshoot the timeout, skipping the post-loop "did the manager ever call stop?" check.
- `CleanupError.model` is now asserted non-empty in tests. Previously the field was an empty `String`, so a cleanup failure whose originating-model context was lost would not be caught in CI. Tests now pin the model name (`qwen/qwen3.6-27b`) so future regressions are caught immediately.

## [1.1.0] - 2026-06-30

### Added

- Minimal first-run onboarding flow. The main window shows three short steps on a fresh install: Groq API key, hotkey, and the minimal overview. The setup state is derived purely from the live Groq key and hotkey status, so the app returns to the matching setup step if the key is cleared or the hotkey becomes invalid. Floe does not call Groq, the microphone, recording, or paste during onboarding. New `OnboardingView`, `GroqSetupStep`, `HotkeySetupStep`, and `OverviewView` components drive the flow. The `lib/setupState` module exposes the pure `computeSetupState` helper.

- Single-instance enforcement via the Tauri 2 single-instance plugin. Secondary launches show and focus the existing main window instead of reinitializing the tray, global hotkey, audio manager, recording, Groq calls, or paste. This makes Start at login plus a later manual launch resolve to a single running instance.

- Groq transcript cleanup as part of the fixed Groq STT → Groq cleanup → clipboard → paste flow. The `cleanup_transcript` command now always sends the Groq transcript through the Groq Chat Completions API and copies the result to the clipboard; if cleanup fails, the raw Groq transcript is pasted instead and a short `Cleanup failed` warning is shown.
- Local pipeline diagnostics for the latest push-to-talk run. The overview now has a small `Diagnostics` action that shows copyable JSON with stage timings, retry counts, audio format metadata, sanitized outcome fields, and the largest local bottleneck. Diagnostics stay in memory only and never include transcripts, cleaned text, raw audio, API keys, auth headers, raw Groq responses, or clipboard contents.
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
- The hardcoded `language: "de"` field from the Groq STT multipart request. Floe no longer forces a STT language; the multipart body now omits the `language` field so Groq Whisper Turbo auto-detects the spoken language.

### Changed

- Switched the Groq production cleanup model to `llama-3.3-70b-versatile`. Qwen cleanup and GPT-OSS cleanup models are not required; cleanup remains Groq-only, non-streaming, and mode-free.
- Refined the recording bubble design and placement for a more recorder-style dictation overlay. The bubble is smaller (≈150×36 pill inside a 170×48 frameless overlay window), solid black with a thin gray border, no translucent or edge-fade effects, no scrollbars, no text/icon/timer/buttons, and only the waveform inside. The pill now sits lower on the work area (≈64px above the bottom edge, down from 96px) while keeping work-area-based bottom-center placement. The waveform shows 11 chunkier bars (3px wide) that scroll right-to-left, each bar representing a 200ms max-pool window of input audio levels so the waveform moves more slowly and preserves a longer recent history. Loud speech creates taller bars, quiet speech creates shorter bars, and silence creates small bars. The local recording pipeline outside the visualization hook is unchanged.
- The main window is gated by an app-level `setupState` (`setup_groq` / `setup_hotkey` / `ready`) derived from the live Groq key and hotkey status. While setup is incomplete, the window renders the new `OnboardingView`. Once both the key is configured and the hotkey is registered, the window shows the minimal `OverviewView` with a `Settings` link.
- `StatusView` is renamed to `OverviewView` and the separate error paragraph is removed. Short status messages (`Hotkey unavailable`, `Cleanup failed`, etc.) are now rendered through the single status line.
- The Settings view's API key section is now labeled `API Key` (singular). Privacy copy, hotkey, and start-at-login sections are unchanged.
- Refactored transcript cleanup to use Groq Chat Completions with `llama-3.3-70b-versatile` and a strict system prompt; cleanup is a single non-streaming call, with bounded retries for network/timeout/429/5xx and respect for `Retry-After`. Only transcript text is sent; audio is never sent for cleanup. GPT-OSS cleanup models are no longer required.
- Cleanup requests now use a bounded word-count-based output limit, and the cleanup command runs asynchronously instead of blocking on the async runtime.
- Optimized the Groq-only transcription pipeline to upload 16 kHz mono 16-bit PCM WAV, send STT requests with `whisper-large-v3-turbo` and `temperature: 0`, reuse a shared HTTP client for Groq calls, validate cleanup output before paste, and capture safe timing/rate-limit metadata in diagnostics. The STT request no longer sends a `language` field, so Groq Whisper Turbo auto-detects the spoken language.
- The main window close button now hides Floe to the system tray instead of quitting the app, so the global push-to-talk hotkey remains registered and active while the window is hidden. Use the new tray `Quit` menu item to fully exit Floe. The tray menu now offers `Show Floe`, `Hide Floe`, `Settings`, and `Quit`. Closing the window while recording continues to record and does not interrupt the audio stream; closing while quitting first unregisters the hotkey and stops recording safely through the existing shutdown path.
- Simplified the desktop UI to a minimal, light, black-and-white design: a first-run onboarding flow, an overview showing only the wordmark, current state, current hotkey, and a `Settings` link, plus a settings view with `API Key`, `Hotkey`, `Start at login`, and `Privacy` sections.
- `AppState` now includes `ready` and `copied` so the status view can show `Ready` and `Copied` instead of internal "Idle" and "Needs attention" labels.
- Privacy note in Settings now reads `Audio → Groq` and `Text → Groq`, reflecting the single-provider flow.
- The default push-to-talk hotkey is now `Control+Space` (shown as `Ctrl + Space`) on Windows/Linux and `Alt+Space` (shown as `Option + Space`) on macOS. Labels use a `Ctrl + Space` style format with spaces and `Ctrl`/`Option` modifier names. The `Control+Space` blocklist entry was removed so the new default can register on Windows/Linux. Legacy saved hotkeys (e.g. `Control+Shift+Space`) remain valid and are loaded unchanged; use `Reset default` in Settings to adopt the new platform default.
- Migrated the bundle identifier (and matching platform keyring service) from `com.floe.app` to `dev.floe.desktop`. The Groq API key is automatically migrated from the legacy keyring service to the new one on first launch via `settings::migrate_legacy_keyring_entries()`, which runs before the `SettingsManager` is constructed. The `.app` suffix warning documented in `BUILD.md` no longer applies. `tauri.conf.json` now carries full bundle metadata (publisher, category, short/long description, homepage, license, copyright, NSIS install mode and languages, macOS minimum system version, Linux `.deb` dependencies and section) so installers populate Add/Remove Programs, Get Info, and Linux package metadata correctly.
- Added a GitHub Actions release pipeline in `.github/workflows/release.yml` that produces a draft Windows NSIS installer for `v*` tags and `workflow_dispatch`. The job runs the same pre-build quality checks as CI, warns when signing secrets are missing, and uses `tauri-apps/tauri-action@v0` to build, sign (when `WINDOWS_CERTIFICATE`/`TAURI_SIGNING_PRIVATE_KEY` are set), and attach the `.exe` to a draft release. `dependabot.yml` now also tracks `github-actions` updates.
