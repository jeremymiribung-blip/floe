# Changelog

All notable changes to Floe will be documented in this file.

## [Unreleased]

### Added

- Minimal recording bubble: a frameless, always-on-top overlay with a volume-reactive audio bar visualization, shown only while recording. Amplitude is derived locally from the same microphone stream that feeds Groq STT; no extra microphone capture and no audio is sent to any provider.
- Initial setup-only Tauri 2, React, TypeScript, Vite, and Rust scaffold.
- Minimal Floe UI with a status indicator, settings placeholder, and manual-test placeholder buttons.
- Stub-only Tauri commands for app status, settings, and manual-test checks.
- Clipboard write and paste automation commands for the manual transcription flow.
- Optional cleanup modes: Raw, Fast, and Cerebras-powered Clean.
- Separate Cerebras API key storage and masked key status in settings.
- Transcript cleanup command with Fast fallback warnings when Clean cleanup fails.
- Reliable configurable global push-to-talk hotkey registration with startup fallback and settings controls.
- Optional Start at login setting that launches Floe hidden in the background, creates the tray icon, and registers the global hotkey after user login.
- Repository docs, issue templates, PR template, MIT license, `.env.example`, `.gitignore`, and GitHub Actions CI.

### Changed

- The main window close button now hides Floe to the system tray instead of quitting the app, so the global push-to-talk hotkey remains registered and active while the window is hidden. Use the new tray `Quit` menu item to fully exit Floe. The tray menu now offers `Show Floe`, `Hide Floe`, `Settings`, and `Quit`. Closing the window while recording continues to record and does not interrupt the audio stream; closing while quitting first unregisters the hotkey and stops recording safely through the existing shutdown path.

### Changed

- Simplified the desktop UI to a minimal, light, black-and-white, two-view design: a status view showing only the wordmark, current state, current hotkey, and a `Settings` link, plus a settings view with `API Keys`, `Hotkey`, and `Privacy` sections.
- Removed the manual recording/testing panel, the cleanup mode selector, the settings summary grid, the colored status pill, and the helper copy from the UI. Behavior and backend commands are unchanged.
- `AppState` now includes `ready` and `copied` so the status view can show `Ready` and `Copied` instead of internal "Idle" and "Needs attention" labels.
