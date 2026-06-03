# Changelog

All notable changes to Floe will be documented in this file.

## [Unreleased]

### Added

- Initial setup-only Tauri 2, React, TypeScript, Vite, and Rust scaffold.
- Minimal Floe UI with a status indicator, settings placeholder, and manual-test placeholder buttons.
- Stub-only Tauri commands for app status, settings, and manual-test checks.
- Clipboard write and paste automation commands for the manual transcription flow.
- Optional cleanup modes: Raw, Fast, and Cerebras-powered Clean.
- Separate Cerebras API key storage and masked key status in settings.
- Transcript cleanup command with Fast fallback warnings when Clean cleanup fails.
- Reliable configurable global push-to-talk hotkey registration with startup fallback and settings controls.
- Repository docs, issue templates, PR template, MIT license, `.env.example`, `.gitignore`, and GitHub Actions CI.
