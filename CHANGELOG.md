# Changelog

All notable changes to Floe will be documented in this file.

## [Unreleased]

### Added

- Minimal first-run onboarding flow. The main window shows three short steps on a fresh install: Groq API key, hotkey, and the minimal overview. The setup state is derived purely from the live Groq key and hotkey status, so the app returns to the matching setup step if the key is cleared or the hotkey becomes invalid. Floe does not call Groq, the microphone, recording, or paste during onboarding. New `OnboardingView`, `GroqSetupStep`, `HotkeySetupStep`, and `OverviewView` components drive the flow. The `lib/setupState` module exposes the pure `computeSetupState` helper.

- Single-instance enforcement via the Tauri 2 single-instance plugin. Secondary launches show and focus the existing main window instead of reinitializing the tray, global hotkey, audio manager, recording, Groq calls, or paste. This makes Start at login plus a later manual launch resolve to a single running instance.

- Groq transcript cleanup as part of the fixed Groq STT → Groq cleanup → clipboard → paste flow. The `cleanup_transcript` command now always sends the Groq transcript through the Groq Chat Completions API and copies the result to the clipboard; if cleanup fails, the raw Groq transcript is pasted instead and a short `Cleanup failed` warning is shown.
- Local pipeline diagnostics for the latest push-to-talk run. The overview now has a small `Diagnostics` action that shows copyable JSON with stage timings, retry counts, audio format metadata, sanitized outcome fields, and the largest local bottleneck. Diagnostics stay in memory only and never include transcripts, cleaned text, raw audio, API keys, auth headers, raw Groq responses, or clipboard contents.
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

- Refined the recording bubble design and placement for a more recorder-style dictation overlay. The bubble is smaller (≈150×36 pill inside a 170×48 frameless overlay window), solid black with a thin gray border, no translucent or edge-fade effects, no scrollbars, no text/icon/timer/buttons, and only the waveform inside. The pill now sits lower on the work area (≈64px above the bottom edge, down from 96px) while keeping work-area-based bottom-center placement. The waveform shows 11 chunkier bars (3px wide) that scroll right-to-left, each bar representing a 200ms max-pool window of input audio levels so the waveform moves more slowly and preserves a longer recent history. Loud speech creates taller bars, quiet speech creates shorter bars, and silence creates small bars. The local recording pipeline outside the visualization hook is unchanged.
- The main window is gated by an app-level `setupState` (`setup_groq` / `setup_hotkey` / `ready`) derived from the live Groq key and hotkey status. While setup is incomplete, the window renders the new `OnboardingView`. Once both the key is configured and the hotkey is registered, the window shows the minimal `OverviewView` with a `Settings` link.
- `StatusView` is renamed to `OverviewView` and the separate error paragraph is removed. Short status messages (`Hotkey unavailable`, `Cleanup failed`, etc.) are now rendered through the single status line.
- The Settings view's API key section is now labeled `API Key` (singular). Privacy copy, hotkey, and start-at-login sections are unchanged.
- Refactored transcript cleanup to use Groq Chat Completions with `llama-3.1-8b-instant` and a strict system prompt; cleanup is a single non-streaming call, with bounded retries for network/timeout/429/5xx and respect for `Retry-After`. Only transcript text is sent; audio is never sent for cleanup. GPT-OSS cleanup models are no longer required.
- Cleanup requests now use a bounded word-count-based output limit, and the cleanup command runs asynchronously instead of blocking on the async runtime.
- Optimized the Groq-only transcription pipeline to upload 16 kHz mono 16-bit PCM WAV, send STT requests with `whisper-large-v3-turbo`, `temperature: 0`, and default language `de`, reuse a shared HTTP client for Groq calls, validate cleanup output before paste, and capture safe timing/rate-limit metadata in diagnostics.
- The main window close button now hides Floe to the system tray instead of quitting the app, so the global push-to-talk hotkey remains registered and active while the window is hidden. Use the new tray `Quit` menu item to fully exit Floe. The tray menu now offers `Show Floe`, `Hide Floe`, `Settings`, and `Quit`. Closing the window while recording continues to record and does not interrupt the audio stream; closing while quitting first unregisters the hotkey and stops recording safely through the existing shutdown path.
- Simplified the desktop UI to a minimal, light, black-and-white design: a first-run onboarding flow, an overview showing only the wordmark, current state, current hotkey, and a `Settings` link, plus a settings view with `API Key`, `Hotkey`, `Start at login`, and `Privacy` sections.
- `AppState` now includes `ready` and `copied` so the status view can show `Ready` and `Copied` instead of internal "Idle" and "Needs attention" labels.
- Privacy note in Settings now reads `Audio → Groq` and `Text → Groq`, reflecting the single-provider flow.
- The default push-to-talk hotkey is now `Control+Space` (shown as `Ctrl + Space`) on Windows/Linux and `Alt+Space` (shown as `Option + Space`) on macOS. Labels use a `Ctrl + Space` style format with spaces and `Ctrl`/`Option` modifier names. The `Control+Space` blocklist entry was removed so the new default can register on Windows/Linux. Legacy saved hotkeys (e.g. `Control+Shift+Space`) remain valid and are loaded unchanged; use `Reset default` in Settings to adopt the new platform default.
