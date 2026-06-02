# Contributing

## Setup

Install Node.js and Rust stable. Use pnpm through Corepack if a standalone `pnpm` command is unavailable:

```powershell
corepack pnpm install
```

## Branches

Use clear branch names such as `codex/feature-name` or `fix/short-bug-name`.

## Commits

Use conventional commit messages:

- `chore: initialize Floe app`
- `feat: add secure settings storage`
- `test: add cleanup tests`
- `ci: add GitHub Actions checks`

## Tests

Run relevant frontend and Rust checks before opening a PR:

```powershell
corepack pnpm run format
corepack pnpm run lint
corepack pnpm run test
corepack pnpm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Tests must not call Groq, require real API keys, or require a real microphone in CI.

## Secrets

Never commit API keys, auth headers, private transcripts, logs with sensitive content, or temporary audio files.

Keep secret and non-secret settings separate. Do not log raw transcripts, raw audio, full API keys, or auth headers.

## V1 Boundaries

V1 uses exactly one Groq Speech-to-Text request after recording stops. Do not add streaming, chunking, realtime partial transcripts, transcript merging, or LLM cleanup.

## Pull Requests

Keep PRs focused. Include what changed, how it was tested, and any known limitations.
