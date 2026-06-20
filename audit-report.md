# FLOE POST-REFACTOR AUDIT

---

## Executive Summary

1. **Local ASR removal is incomplete** — the `asr/` provider abstraction layer survives as dead-weight scaffolding over a single provider (Groq). The abstraction layer (traits, registry, fallback, policy, backend) exists only to support a multi-provider architecture that was removed and never replaced.
2. **Nemotron removal is nearly complete** — the `.venv-nemotron/` directory is a leftover, but no Nemotron Rust code remains. However, `whisper_local` test data strings survive in 4 test files and 1 source file.
3. **The ASR architecture is overengineered for a single-provider app** — ~3,200 lines of provider abstraction, registry, fallback strategy, policy engine, and test files exist only to route audio through the single Groq provider. This is the primary technical debt.
4. **Dead code is pervasive** — `#![allow(dead_code)]` appears in 3 modules (`asr/mod.rs`, `commands/diag.rs`, `contract.rs`). Multiple functions use `#[allow(dead_code)]`. These are documented lies.
5. **Settings are clean** — `AppSettings` contains only `hotkey` and `keyring_migrated`. No leftover provider settings. Legacy `cleanupMode` is gracefully ignored.
6. **Frontend is clean** — no leftover provider UI, no hidden references. Onboarding and settings show only Groq API key and hotkey.
7. **Recording pipeline is solid** — `PushToTalkController` has correct state management, proper release-after-start handling, frontend watchdog, and error differentiation.
8. **Hotkey lifecycle is robust** — startup registration, fallback to default, graceful shutdown, race handling around press/release. The async Tauri event flow is correct.
9. **Error handling has gaps** — many `unwrap()` and `unwrap_or_default()` calls in `lib.rs` setup. Several `.catch(() => {})` swallows on the frontend. `map_err(|_| app_settings_error())` in settings.rs loses error context.
10. **Production readiness is close but not reached** — the codebase works for the single Groq path but carries architectural dead weight that complicates maintenance, increases build times, and obscures the true pipeline.

---

## Cleanup Verification

### Was local ASR removed cleanly?

**No.**

The `asr/` module tree remains intact with all its abstractions. The following modules exist but are only useful if there were multiple providers:

- `asr::traits` — `AsrProvider`, `AsrSession` traits with streaming, partials, session management
- `asr::registry` — `ProviderRegistry` with health cache, experimental/disabled tracking, selection criteria with multiple providers
- `asr::fallback` — `FallbackStrategy` with retry logic, exponential backoff, WAV decoding
- `asr::policy` — `ResourcePolicy` with audio validation
- `asr::backend` — `AsrBackend` that coordinates selection + fallback
- `asr::error` — 6 error types: `AsrError`, `SessionError`, `RegistryError`, `SelectionError`, `AsrErrorCode`, `SessionErrorCode`, `RegistryErrorCode`, `SelectionErrorCode`
- `asr::types` — 6 structs + 3 enums + diagnostics
- `asr::tests` — 5 test files for provider selection, fallback, diagnostics, policy, privacy

Only one adapter exists: `asr::adapters::groq::GroqAdapter`. It wraps the real implementation in `providers::groq::stt::GroqTranscriptionClient`, adding an unnecessary delegation layer.

### Was Nemotron removed cleanly?

**Nearly, but not completely.**

- **Removed correctly**: No Nemotron Rust source, no Nemotron imports, no Nemotron Cargo dependencies
- **What remains**:
  - `D:\Code\FLOE\.venv-nemotron/` — orphaned Python virtual environment directory
  - String `"whisper_local"` appears in:
    - `src-tauri/src/asr/types.rs:260` (test: `AsrDiagnostics::new("whisper_local"...`)
    - `src-tauri/src/asr/tests/diagnostics_tests.rs:57` (test reference)
    - `src-tauri/src/asr/tests/diagnostics_tests.rs:130,135` (test reference)
    - `src-tauri/src/asr/tests/privacy_tests.rs:112` (test reference)
    - `src-tauri/src/commands/diag.rs:270` (test reference)

### What still needs removal?

1. `.venv-nemotron/` — entire directory should be deleted
2. `asr::adapters` — the delegation layer (`GroqAdapter` wrapping `GroqTranscriptionClient`) should be collapsed
3. `asr::traits` — `AsrProvider`, `AsrSession` — no multiple providers exist
4. `asr::registry` — `ProviderRegistry` — no multiple providers to register
5. `asr::fallback` — `FallbackStrategy` — no fallback provider exists
6. `asr::policy` — `ResourcePolicy` — validation could be inline in the transcription command
7. `asr::backend` — `AsrBackend` — unnecessary coordinator
8. `asr::error` — 6 error types when 1-2 would suffice
9. `asr::types` — `AsrDiagnostics`, `BackendType`, `Deployment`, `StreamingSupport`, etc. — these multiplied by provider abstraction
10. `asr/tests/` — all 5 test files test the dead abstraction
11. `whisper_local` string references in test files
12. Runtime log artifacts: `diag_stderr.log`, `diag.txt`, `fe_diag.log`, `inventory.ndjson`, `nul`
13. `libpolicy.rlib` — compiled Rust artifact
14. `ASR_ARCHITECTURE.md` — documents Vosk, Whisper Local, runtime process, model manager — all of which do not exist
15. `src/stores/settings.ts` — dead store (`useSettingsStore`) duplicated by `useFloeStore`

---

## Dead Code Report

| Item                  | Location                                                                | Evidence                                                                                                                                                          |
| --------------------- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dead module tree      | `src-tauri/src/asr/` (10 files, ~2200 LOC + ~1900 LOC tests)            | `#![allow(dead_code)]` on `mod.rs`; single provider doesn't need registry, fallback, traits                                                                       |
| Dead code allow       | `src-tauri/src/contract.rs:12`                                          | `#![allow(dead_code)]` at module level                                                                                                                            |
| Dead code allow       | `src-tauri/src/commands/diag.rs:1`                                      | `#![allow(dead_code)]` at module level                                                                                                                            |
| Dead functions        | `src-tauri/src/settings.rs`                                             | `#[allow(dead_code)]` on: `get_app_settings_async`, `save_app_settings_async`, `restore_settings_from_backup`, `default_empty_string`, `AppSettingsStore` methods |
| Dead functions        | `src-tauri/src/recording/mod.rs`                                        | `#[allow(dead_code)]` on: `set_state_and_emit`, `state_arc`, `poll_finalize`                                                                                      |
| Dead store            | `src/stores/settings.ts`                                                | `useSettingsStore` with `isCapturingHotkey` — duplicated by `useFloeStore.isHotkeyCaptureActive`                                                                  |
| Dead dependency       | `src-tauri/Cargo.toml`                                                  | `async-trait` exists solely for the dead `AsrProvider`/`AsrSession`/`CleanupProvider` traits; also used in test_helpers                                           |
| Whisper_local strings | 4 test files + 1 source file                                            | `"whisper_local"` in `asr/types.rs`, `asr/tests/diagnostics_tests.rs`, `asr/tests/privacy_tests.rs`, `commands/diag.rs`                                           |
| Orphaned directory    | `D:\Code\FLOE\.venv-nemotron/`                                          | Old Python venv for Nemotron experiments                                                                                                                          |
| Orphaned docs         | `D:\Code\FLOE\docs\ASR_ARCHITECTURE.md`                                 | Documents Vosk, Whisper Local, runtime process — none exist in code                                                                                               |
| Runtime artifacts     | Root directory                                                          | `diag_stderr.log`, `diag.txt`, `fe_diag.log`, `inventory.ndjson`, `nul`                                                                                           |
| Build artifact        | `D:\Code\FLOE\libpolicy.rlib`                                           | Compiled Rust library artifact                                                                                                                                    |
| Empty test bodies     | `src-tauri/src/commands/recording.rs:114-118`, `bubble.rs:91-95,97-103` | Tests that only document that functions compile                                                                                                                   |

---

## Architecture Scorecard

### Rust Architecture: **45/100**

The Rust backend is split between a clean, functional recording pipeline and a legacy, overengineered ASR abstraction layer that was designed for multi-provider support but now only routes to Groq. The `asr/` module accounts for ~2200 lines of production code plus ~1900 lines of test code — roughly 40% of the Rust codebase — all of which is scaffolding for a scenario (multiple ASR providers) that was removed. The dichotomy between `asr::adapters::groq::GroqAdapter` (trait-based) and `providers::groq::stt::GroqTranscriptionClient` (direct implementation) is a layering violation: the adapter wraps the client but adds no value. The `providers::cleanup::CleanupProvider` trait is only implemented by `GroqCleanupClient` — it's an abstraction for one implementation.

### Tauri Architecture: **70/100**

Commands are well-structured in `commands/`. State management via `app.manage()` is correct. The `contract.rs`/`contract.ts` mirror pattern is disciplined. `lib.rs` has some brittle `unwrap()` calls in `setup()` that would crash on any failure. The invoke handler list is long but correct. Bubble overlay window management via events is solid. Single-instance plugin is correctly configured.

### Recording Pipeline: **80/100**

`RecordingManager` is the strongest module. It handles start/stop, watchdog timeout, device disconnect, shutdown, concurrent access, and race conditions between stop and watchdog. Tests are comprehensive (concurrent start/stop, shutdown, watchdog races, buffer poisoning recovery). The known race documented in `mod.rs` is acceptable with the described mitigation. The `PushToTalkController` on the frontend correctly handles release-before-start, frontend watchdog, and error differentiation.

### Hotkey Lifecycle: **75/100**

Startup registration with fallback to default hotkey is correct. The `HotkeyManager` trait abstraction (`HotkeyRegistrar`) is overengineered for one real implementation (Tauri) plus one test fake. The `register_or_fallback` method is well-designed. Event emission via `run_on_main_thread` is correct for Tauri's threading model. Unregistration on shutdown is clean. The `can_accept_commands()` guard prevents hotkey events during shutdown.

### Error Handling: **55/100**

Uneven. Upstream errors in `lib.rs` use `.unwrap()` (line 62: `unwrap_or_default()`, line 88: `unwrap()` on `settings_manager.get_api_key_secret()`, etc.). The `settings.rs` module uses `map_err(|_| app_settings_error())` repeatedly, discarding the original error context. Frontend uses `.catch(() => {})` in multiple places (`tauri.ts` lines 141, 147, 158; `App.tsx` lines 23, 33, 39, 40, 53). The `recordingErrors.ts` and `clipboardErrors.ts` patterns are good. Pipeline errors are correctly propagated through `PushToTalkController`.

### State Management: **80/100**

Zustand stores are minimal and focused. `useFloeStore` correctly separates pipeline state from config state. The `syncFromPipeline` mechanism is clean. Dedup: `src/stores/settings.ts` is dead (its `isCapturingHotkey` is a copy of `useFloeStore.isHotkeyCaptureActive`).

### Test Quality: **70/100**

Strong in `recording/` (comprehensive tests for state transitions, watchdog, races, shutdown). Strong in `settings.rs`. Weak in `asr/tests/` (tests a dead abstraction). Weak in commands (empty test bodies). No integration tests for the full pipeline (hotkey→paste). Test helpers exist (`test_helpers`) with fake ASR providers.

### Maintainability: **50/100**

The `asr/` abstraction adds significant cognitive load. A new contributor must understand: `AsrProvider`/`AsrSession` traits, `ProviderRegistry`, `FallbackStrategy`, `ResourcePolicy`, `AsrBackend`, `SelectionCriteria` — only to discover that all of it routes to a single call to `GroqTranscriptionClient.transcribe_wav()`. The dual client structure (`GroqAdapter` + `GroqTranscriptionClient`) is confusing. Multiple `#![allow(dead_code)]` directives signal code that the authors know is unused but haven't cleaned up.

### Production Readiness: **65/100**

The app will work for the single Groq path. The pipeline is correct. Hotkey lifecycle is robust. But:

- `lib.rs` will crash on startup if certain Tauri APIs fail (multiple `unwrap()` calls)
- The `asr/` dead code increases binary size and attack surface for no benefit
- Error context is lost in several places (`map_err(|_| ...)`)
- Orphaned dependencies (`async-trait` primarily for dead traits)
- Runtime log artifacts in the project root suggest development/testing artifacts weren't cleaned before this audit

---

## Top 10 Weaknesses

1. **ASR abstraction layer is dead weight** — ~4000 lines of traits, registry, fallback, policy, backend, adapters, errors, types, and tests exist only to call one Groq API endpoint. This is the single largest maintainability problem.

2. **`#![allow(dead_code)]` in 3 modules** — documents that code exists but is unused. This is a hygiene failure: either delete the code or use it.

3. **Multiple `unwrap()` calls in `lib.rs` startup** — `lib.rs:62,88` will panic on failure. Production code should handle all startup errors gracefully.

4. **Error context discarded via `map_err(|_| ...)`** — `settings.rs` loses the original error in at least 8 places. This makes debugging impossible.

5. **Frontend error swallowing** — `.catch(() => {})` in `tauri.ts:141,147,158`, `App.tsx:23,33,39,40,53` silently drops errors that could indicate real problems.

6. **Orphaned `asr::adapters` layer** — `GroqAdapter` wraps `GroqTranscriptionClient` with no additional value. Two implementations of the same thing.

7. **Dead `src/stores/settings.ts`** — `useSettingsStore` is never imported by any component. Its `isCapturingHotkey` duplicates `useFloeStore`.

8. **Runtime artifacts in project root** — `diag.txt`, `diag_stderr.log`, `fe_diag.log`, `inventory.ndjson`, `nul` should be in `.gitignore` or removed.

9. **`async-trait` Cargo dependency for dead code** — the primary consumer of `async-trait` is the dead `AsrProvider`/`AsrSession` traits. Removing the dead code removes the dependency.

10. **`ASR_ARCHITECTURE.md` documents features that don't exist** — Vosk adapter, Whisper Local adapter, runtime process, model manager. This is misleading documentation.

---

## Top 10 Strengths

1. **Recording lifecycle is production-quality** — `RecordingManager` has comprehensive race condition handling, watchdog, device disconnect, and shutdown support with excellent test coverage.

2. **Pipeline orchestration is correct** — `PushToTalkController` correctly handles release-before-start, frontend watchdog, error differentiation per stage, and diagnostics capture.

3. **Hotkey lifecycle is robust** — startup registration, fallback to default, shutdown unregistration, `can_accept_commands()` guard, and `register_or_fallback` are well-designed.

4. **Settings are clean** — `AppSettings` contains only what's needed. Legacy settings are gracefully ignored with test coverage.

5. **Frontend-backend contract discipline** — `contract.rs` / `contract.ts` mirror with test verification of command names. This is excellent architecture hygiene.

6. **Privacy controls are strong** — `assertDiagnosticsSafe` with forbidden keys list, `sanitize_error_code`, `sanitizeDiagnosticCode`. The system actively prevents leaking sensitive data.

7. **WAV encoding is well-tested** — `encode_pcm16_wav`, `encode_recording_wav`, resampling, AGC all have thorough unit tests.

8. **Cleanup fallback is correct** — `cleanup_transcript_with` falls back to raw transcript on any failure with a `"Cleanup failed"` warning, as specified in AGENTS.md.

9. **State management is minimal and focused** — Zustand stores are small, well-typed, with clear separation of pipeline state and configuration state.

10. **Module organization is logical** — `commands/`, `providers/`, `recording/`, `system/`, `diag/` directories have clear responsibilities. The separation of secret/non-secret settings is correctly implemented.

---

## Top 10 Refactoring Priorities

1. **Collapse the `asr/` module tree** — Replace `AsrBackend.transcribe()` with a direct call to `GroqTranscriptionClient.transcribe_wav()` in `commands/transcription.rs`. Remove: `asr/traits.rs`, `asr/registry.rs`, `asr/fallback.rs`, `asr/policy.rs`, `asr/backend.rs`, `asr/adapters/groq.rs`, `asr/adapters/mod.rs`, `asr/tests/`. Keep `asr/types.rs` only if `AsrDiagnostics` is still needed elsewhere (it is used in `commands/transcription.rs`).

2. **Remove `#![allow(dead_code)]` directives** — After collapsing the `asr/` module, remove dead code from `contract.rs`, `commands/diag.rs`, `settings.rs`, `recording/mod.rs`.

3. **Replace `unwrap()` calls in `lib.rs` setup** — Convert `unwrap()` on `settings_manager.get_api_key_secret()` and `app.path().app_config_dir()` to proper error handling.

4. **Fix error swallowing** — Remove `.catch(() => {})` in `tauri.ts` and `App.tsx`. Log or display errors appropriately.

5. **Replace `map_err(|_| ...)` with context-preserving errors** — In `settings.rs`, preserve original error messages. Use `anyhow` or a helper that wraps errors with context.

6. **Delete `.venv-nemotron/`** — orphaned virtual environment.

7. **Remove orphaned `whisper_local` test references** — Update test strings that reference `whisper_local` to use `groq` or a generic provider name.

8. **Delete `src/stores/settings.ts`** — dead store, never imported.

9. **Clean up `ASR_ARCHITECTURE.md`** — either rewrite to match current single-provider reality or delete it.

10. **Add `.gitignore` entries** — for runtime artifacts (`diag.txt`, `diag_stderr.log`, `fe_diag.log`, `inventory.ndjson`, `nul`) and build artifacts (`libpolicy.rlib`).

---

## Principal Engineer Verdict

### Significant Cleanup Still Required

The single most important finding is that the `asr/` module tree (~2200 lines of production code + ~1900 lines of test code) is scaffolding for a multi-provider architecture that was removed but never dismantled. This is not speculative — every file in `asr/` except possibly `types.rs` exists only to support a scenario with multiple ASR providers, but only one provider (Groq) exists. The codebase has `#![allow(dead_code)]` in 3 modules and `#[allow(dead_code)]` on 7+ individual functions, which is a documented acceptance of technical debt.

The codebase works correctly for its single Groq path, and the core pipeline (recording, hotkey, cleanup, paste) is well-engineered. But the dead abstraction layer is a genuine liability: it increases binary size, complicates every build, adds cognitive overhead for every contributor, and creates a false impression of provider neutrality that AGENTS.md explicitly rejects.

The cleanup is straightforward: delete the asr abstraction, route directly to `GroqTranscriptionClient`, and remove the dead code annotations. This would eliminate ~4000 lines of dead code and reduce the Rust codebase by approximately 40%.

**This is the highest-impact refactoring available in this codebase and should be done before any new feature work.**
