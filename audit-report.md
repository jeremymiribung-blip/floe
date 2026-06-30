# FLOE Production Audit — June 29, 2026

---

## Executive Summary

The previous audit identified the `asr/` module tree (~4,000 lines of dead abstraction code) as the single largest liability. **That module has been completely deleted.** The codebase is substantially healthier and more focused. The core pipeline (recording → STT → cleanup → clipboard → paste) is production-quality with strong test coverage. However, new issues have emerged:

- **Onboarding flow is entirely missing** despite being specified in AGENTS.md — the app has no setup gating, no guided first-run experience.
- **Error swallowing persists** in 12+ frontend locations, silently dropping failures that could indicate real problems.
- **24 `#[allow(dead_code)]` annotations** remain scattered across the codebase, down from 3 module-level + 7 function-level but still a hygiene concern.
- **4 dead UI components** (`Button`, `Card`, `Tabs`, `Separator`) are compiled into the frontend bundle but never rendered.

**Verdict: Very Close to Production Ready.** The architectural rot that plagued the previous audit has been surgically removed. What remains are surface-level issues — missing UX flows, error handling discipline, and dead code cleanup — all of which are individually small fixes.

---

## Previous Findings — Validation

| # | Finding | Status | Evidence |
|---|---------|--------|----------|
| 1 | ASR abstraction layer dead weight (~4,000 lines) | **Fixed** | `asr/` directory tree deleted entirely. No `asr/mod.rs`, no `asr/traits.rs`, no `asr/registry.rs`, no `asr/fallback.rs`, no `asr/policy.rs`, no `asr/backend.rs`, no `asr/adapters/`, no `asr/tests/`. |
| 2 | `#![allow(dead_code)]` in 3 modules | **Partially Fixed** | Down from 3 module-level + 7 function-level to 1 module-level (`contract.rs:12`) + 23 item-level annotations across `diag/`, `recording/`, `commands/diag.rs`, `providers/groq/types.rs`, `test_helpers/`, `settings.rs` (now uses `#[expect]`). Still 24 total — significant reduction, but not eliminated. |
| 3 | `unwrap()` calls in `lib.rs` startup | **Fixed** | Zero `unwrap()`, `unwrap_or_default()`, or `expect()` calls remain. Uses `unwrap_or_else()` and `?` operator. |
| 4 | Error context discarded via `map_err(\|_\| ...)` | **Fixed** | Zero `map_err(\|_\|` patterns remain. All use named functions like `map_keyring_error` and `log_then_settings_error`. |
| 5 | Frontend error swallowing | **Partially Fixed** | Previously identified locations in `tauri.ts` and `App.tsx` removed/changed. However, 12+ `.catch(() => {})` or `console.error`-only swallow sites remain across `usePushToTalk.ts`, `pushToTalk.ts`, `tauri.ts`, `UpdateSection.tsx`. |
| 6 | Orphaned `asr::adapters` layer | **Fixed** | Module deleted. No `GroqAdapter` type exists. |
| 7 | Dead `src/stores/settings.ts` | **Fixed** | File deleted. Zero references to `useSettingsStore` or `settingsStore` exist. |
| 8 | Runtime artifacts in project root | **Mostly Fixed** | `nul` file no longer present on disk. `diag.txt`, `diag_stderr.log`, `fe_diag.log`, `inventory.ndjson`, `libpolicy.rlib` all cleaned up. `.gitignore` updated with all artifact entries (lines 39–46). |
| 9 | `async-trait` Cargo dependency for dead code | **No Longer Relevant** | `async-trait` remains in Cargo.toml (line 23) but is now **actively used** by the `CleanupProvider` trait and its implementations. Not dead. |
| 10 | `ASR_ARCHITECTURE.md` documents features that don't exist | **Fixed** | Document deleted. Entire `docs/` directory removed. |

---

## New Findings

### Finding 1: Missing Onboarding Flow
**Severity:** High  
**Category:** Feature Completeness  
**Files:** `src/views/SettingsWindow.tsx`, `src/types/app.ts:22-26`, `src/Root.tsx`  
**Evidence:** AGENTS.md specifies a `setupState` gating mechanism (`setup_groq`, `setup_hotkey`, `ready`) that drives an onboarding flow. The `AppStatus` type defines `status: "setup_only"` (line 24). **Neither the gating logic nor any onboarding UI exists.** `SettingsWindow.tsx` renders all settings unconditionally. A user who launches Floe without a configured API key or hotkey sees a fully functional-looking settings window with no guidance, no warnings, and no forced setup path. The `setup_only` state is never referenced by any component or hook.  
**Why it matters:** First-run experience is broken. Users are dropped into an empty settings panel with no onboarding context, no call-to-action, and no indication of what they need to do.  
**Suggested fix:** Implement the setupState gating in `Root.tsx`: check backend on mount for API key and hotkey status; if either is missing, show an onboarding step rather than the full settings panel. Add a guided "Enter your API key" → "Configure hotkey" → "Ready to use" flow.  
**Effort:** Medium

### Finding 2: Frontend Error Swallowing
**Severity:** High  
**Category:** Error Handling  
**Files:** 
- `src/hooks/usePushToTalk.ts:115,118` — `.catch(() => {})` on window show/focus
- `src/hooks/usePushToTalk.ts:172` — `.catch(() => { ... setError })` no catch argument
- `src/hooks/usePushToTalk.ts:232` — same pattern for recording events
- `src/lib/pushToTalk.ts:180` — `.catch((e) => { diagLog(...) })` no user feedback
- `src/lib/pushToTalk.ts:247-249` — `catch { /* Backend reset failed... */ }`
- `src/lib/pushToTalk.ts:560-562` — `catch { /* status refresh should not block */ }`
- `src/lib/pushToTalk.ts:582` — `.catch(() => { /* Best-effort */ })`
- `src/lib/tauri.ts:171-173` — `.catch((err) => { diagLog(...) })`
- `src/lib/tauri.ts:177-179` — `.catch((err) => { diagLog(...) })`
- `src/lib/tauri.ts:189-191` — `.catch((err) => console.error(...))`
- `src/hooks/useRollingWaveform.ts:45-47` — `.catch(() => {})`
- `src/components/UpdateSection.tsx:153-155` — `.catch((err) => console.error(...))`
- `src/components/UpdateSection.tsx:160-162` — `.catch((err) => console.error(...))`
- `src/App.tsx:31,42,51,55,65,76,79,94` — all `.catch((err) => console.error(...))`

**Evidence:** 22 distinct .catch() sites. Of these, 5 use completely empty handlers (`catch {}` / `.catch(() => {})`), 10 log to `diagLog` or `console.error` without user feedback, and 7 show error state but lose the error object. Startup failures (`getHotkeySettings`, `getApiKeyStatus`, `getUpdateInfo`) are silently logged — if a keychain error prevents loading the API key, the app shows "no key configured" with no indication of the underlying problem.  
**Why it matters:** Silent failures hide real problems — keychain corruption, missing permissions, network errors — making debugging nearly impossible for end users.  
**Suggested fix:** 
1. Never use empty `.catch(() => {})` — at minimum log with `console.error`.
2. Surface startup failures to the user via store state (e.g., `setApiKeyError` action).
3. For non-critical operations (diagLog, bubble show/hide), log is acceptable; for critical operations (hotkey registration, recording control), propagate errors.  
**Effort:** Medium

### Finding 3: Dead UI Components
**Severity:** Medium  
**Category:** Dead Code / Technical Debt  
**Files:**
- `src/components/ui/button.tsx` — never imported
- `src/components/ui/card.tsx` — never imported
- `src/components/ui/tabs.tsx` — never imported
- `src/components/ui/separator.tsx` — never imported
- `src/lib/shadcn-theme.css` (498 lines) — CSS for these components is orphaned

**Evidence:** Grep for `from.*ui/(button|card|tabs|separator)` across `src/` returns zero matches. The `SettingsWindow` uses raw `<button>` elements, not the `Button` component. The corresponding CSS selectors (`[data-slot="button"]`, `[data-slot="card"]`, `[role="tab"]`, `[role="separator"]`) exist in `shadcn-theme.css` but are never triggered. Only `Input`, `Label`, and `Switch` from `ui/` are actively used.  
**Why it matters:** 4 unused components and ~500 lines of orphaned CSS increase bundle size and cognitive load. A contributor might try to use `Button` only to discover it has never been rendered and may have styling issues.  
**Suggested fix:** Delete the 4 unused component files and the orphaned CSS selectors. If needed later, they can be recreated from the shadcn/ui source.  
**Effort:** Small

### Finding 4: `#[allow(dead_code)]` Proliferation
**Severity:** Medium  
**Category:** Code Hygiene / Technical Debt  
**Files:** 24 occurrences across:
- `src-tauri/src/contract.rs:12` — `#![allow(dead_code)]` (module-level, justified for CMD_* constants)
- `src-tauri/src/commands/diag.rs:45,99` — `#[allow(dead_code)]` on `set_path`, `new`
- `src-tauri/src/recording/mod.rs:467,537` — `#[allow(dead_code)]` on `state_arc`, `poll_finalize`
- `src-tauri/src/diag/report.rs:182,374,873` — `#[allow(dead_code)]`
- `src-tauri/src/diag/event.rs:6` — `#[allow(dead_code)]`
- `src-tauri/src/diag/mod.rs:50` — `#[allow(dead_code)]`
- `src-tauri/src/diag/tracer.rs:41,60,68,115,117,119,124,140` — 8 `#[allow(dead_code)]`
- `src-tauri/src/recording/error.rs:38,46` — `#[allow(dead_code)]`
- `src-tauri/src/providers/groq/types.rs:45,104` — `#[allow(dead_code)]`
- `src-tauri/src/test_helpers/cleanup.rs:31` — `#[allow(dead_code)]`

**Evidence:** 1 module-level + 23 item-level annotations. The diag/tracer.rs file alone has 8 annotations, suggesting code that was written speculatively. The `settings.rs` file has been upgraded to `#[expect(dead_code)]` (the better form) on 2 functions, but the rest still uses the suppress-everything `#[allow]` form.  
**Why it matters:** Each `#[allow(dead_code)]` is a documented admission that code exists without being used. This makes refactoring harder (you can't tell if removing something breaks a "dead" function that's actually called dynamically) and signals uncertainty about what's needed.  
**Suggested fix:** 
1. Convert `#[allow(dead_code)]` → `#[expect(dead_code)]` where the code is intentionally dead (test helpers, pub API surface for future use).
2. Where possible, gate with `#[cfg(test)]` instead of suppressing warnings.
3. Remove genuinely unused code.  
**Effort:** Medium

### Finding 5: `CleanupProvider` Single-Implementation Trait
**Severity:** Low  
**Category:** Architecture  
**Files:** `src-tauri/src/providers/cleanup.rs:39-46`, `src-tauri/src/providers/groq/cleanup.rs:186-188`  
**Evidence:** `CleanupProvider` is a trait with exactly one production implementation (`GroqCleanupClient`). Two test implementations exist (`FakeCleanupProvider`, `TrackedCleanupProvider`). The trait provides an abstraction boundary that could theoretically support a second provider, but AGENTS.md explicitly disallows provider switching: *"No Qwen cleanup model or GPT-OSS cleanup model is required."* This is the same pattern as the (now-deleted) `AsrProvider` trait, but on a much smaller scale (~46 lines for the trait definition + ~1 production impl).  
**Why it matters:** Minor architectural overdraft. The trait exists to support a scenario that is explicitly ruled out by project constraints. However, unlike the old `asr/` module, this trait is lean, well-documented, and serves a clear testability purpose.  
**Suggested fix:** Keep as-is. The testability benefit (mock providers) justifies the abstraction. Close as "wontfix."  
**Effort:** N/A

### Finding 6: `HotkeyRegistrar` Single-Implementation Trait
**Severity:** Low  
**Category:** Architecture  
**Files:** `src-tauri/src/system/hotkey.rs:77-80`  
**Evidence:** `HotkeyRegistrar` trait has one production impl (`TauriHotkeyRegistrar`) and one test impl (`FakeRegistrar`). Same pattern as CleanupProvider.  
**Suggested fix:** Keep as-is. Testability benefit is real.  
**Effort:** N/A

### Finding 7: `SecretStore` Trait with Two Implementations
**Severity:** Informational  
**Category:** Architecture  
**Files:** `src-tauri/src/settings.rs:77-81`  
**Evidence:** `SecretStore` trait has two production implementations (`KeyringSecretStore`, `KeyringEntryStore`) plus test implementations. Two production implementations serve different keyring access patterns (one for the main service, one for legacy migration). This is a legitimate use of the trait pattern.  
**Why it matters:** No issue. This is correct abstraction.  
**Suggested fix:** None.  
**Effort:** N/A

### Finding 8: Frontend Test Coverage Gap
**Severity:** Medium  
**Category:** Testing  
**Files:** All `*.test.ts` / `*.test.tsx` under `src/`  
**Evidence:** 13 test files for 17 components + 6 hooks + 20 lib files + stores + views. The `usePushToTalk.ts` hook (288 lines of critical pipeline orchestration) has zero tests. `SettingsWindow.tsx` (647 lines, 20+ interaction paths) has zero tests. `pushToTalk.ts` (623 lines, 10+ states) has zero tests. In contrast, Rust has comprehensive unit and integration tests with a dedicated `PipeHarness` test infrastructure.  
**Why it matters:** The most complex frontend logic — pipeline orchestration, state transitions, error handling, settings interactions — is untested. A regression in `usePushToTalk` or `pushToTalk` would go undetected by CI.  
**Suggested fix:** Add tests for `pushToTalk.ts` (test state machine transitions, watchdog, error paths using fake dependencies) and `usePushToTalk.ts` (test hotkey events, state sync).  
**Effort:** Large

### Finding 9: RPM Spec at Version 0.1.0
**Severity:** Low  
**Category:** Release Readiness  
**File:** `D:\Code\FLOE\floe.spec:2`  
**Evidence:** `%global app_version 0.1.0` — does not match the application version in `package.json` or `Cargo.toml`.  
**Why it matters:** RPM package would report version 0.1.0 regardless of actual app version. The spec also references `https://github.com/user/floe` as the URL, which is a placeholder.  
**Suggested fix:** Derive version from the build system or update to match `package.json`. Fix the URL.  
**Effort:** Small

### Finding 10: `SttProviderDiagnostics` Metadata Skew
**Severity:** Low  
**Category:** Dead Code  
**Files:** `src/types/app.ts:173-180`  
**Evidence:** `SttProviderDiagnostics` interface contains fields like `providerName`, `fallbackUsed` that were part of the old multi-provider ASR abstraction. The field `sttProvider?: SttProviderDiagnostics` exists on `SttResult` (line 187) and `SttError` (line 197), but these fields are never populated by the backend — the Rust `GroqTranscription` struct doesn't include `stt_provider`.  
**Why it matters:** Dead fields in TypeScript types create confusion about what data is actually available. A frontend developer might try to use `sttResult.sttProvider?.providerName` and get `undefined` at runtime.  
**Suggested fix:** Remove `SttProviderDiagnostics` and the `sttProvider` field from `SttResult` and `SttError`, or implement it properly in the backend.  
**Effort:** Small

### Finding 11: No `listener` Cleanup for `listen()` in Tests
**Severity:** Low  
**Category:** Testing  
**Files:** `src/hooks/usePushToTalk.ts:158-183`, `src/App.tsx:59-70,72-84`  
**Evidence:** The `listen()` calls in `usePushToTalk` and `App.tsx` properly clean up via returned unlisten functions. But if these components are used in test environments without Tauri, the `listen()` import may throw or hang. The `isTauriRuntime()` guard protects against this, but test coverage of these effects is absent.  
**Why it matters:** Tests could hang or fail mysteriously if Tauri event listeners aren't mocked.  
**Suggested fix:** Not actionable without test coverage. Would be resolved by Finding 8's test improvements.  
**Effort:** Small (consequence of Finding 8)

### Finding 12: Update System Error Paths
**Severity:** Medium  
**Category:** Error Handling  
**Files:** `src/components/UpdateSection.tsx:153-155,160-162`, `src/App.tsx:49-56`  
**Evidence:** `installUpdate().catch((err) => console.error(...))` and `resetUpdateState().catch((err) => console.error(...))` silently log update failures. `checkForUpdate()` failures are consumed with `.catch(console.error)` at app startup. A failed update check is never surfaced to the user.  
**Why it matters:** If the update server is unreachable or the update download fails, the user sees only "You're up to date" or a stale version with no indication of a problem.  
**Suggested fix:** Propagate update check/download/install errors to `UpdateInfo.errorMessage` so the UI can display them.  
**Effort:** Small

---

## Scoring

| Category | Score | Explanation |
|----------|-------|-------------|
| Feature Completeness | 82/100 | Core pipeline complete. Missing onboarding flow. Dead `AppStatus.setup_only` type. |
| Architecture | 90/100 | ASR dead weight removed. Clean module boundaries. Minor over-abstraction (CleanupProvider, HotkeyRegistrar traits) but justifiable for testability. |
| Rust Quality | 85/100 | Solid code quality, idiomatic Rust. 24 dead_code annotations are the main blemish. |
| Frontend Quality | 78/100 | Clean store design. Dead UI components. Missing onboarding. Thin test coverage. Error swallowing. |
| Tauri Integration | 88/100 | Commands well-structured. State management correct. No more `unwrap()` in setup. Event system solid. |
| IPC | 88/100 | Contract mirror well-maintained. CamelCase enforced. 37 commands registered and tested. |
| Recording Pipeline | 92/100 | Production-quality. Comprehensive race handling. Excellent tests. |
| Hotkey Lifecycle | 87/100 | Robust registration with fallback. Clean shutdown. Trait abstraction justifiable. |
| Cleanup Pipeline | 88/100 | Clean separation, correct fallback, output validation, provider-agnostic tests. |
| Diagnostics | 90/100 | Comprehensive, PII-free, crash detection, session persistence. One of the strongest subsystems. |
| Update System | 72/100 | Functional via Tauri updater plugin. Error reporting is weak — failures silently logged. |
| Error Handling | 68/100 | Much improved from previous audit (55). No more `unwrap()` or `map_err(\|_\|)`. But 22 frontend `.catch()` sites swallow errors. Update errors never surfaced. |
| Security | 92/100 | OS keychain for secrets. No secrets in logs. Clipboard read-back verification. |
| Privacy | 94/100 | Strong redaction system. Forbidden keys in diagnostics. PII-free report design. |
| Performance | 86/100 | In-memory WAV, no unnecessary I/O. Dead code adds minor binary bloat. |
| Resource Management | 88/100 | Clean shutdown, watchdog, device disconnect handling, mutex poisoning recovery. |
| Test Quality | 78/100 | Excellent Rust tests (unit + integration + contract). Weak frontend tests — critical hooks and views untested. No E2E tests. |
| Documentation | 90/100 | AGENTS.md accurate. BUILD/CHANGELOG/CONTRIBUTING/README all up to date. ASR_ARCHITECTURE.md deleted. |
| Release Readiness | 72/100 | CI/CD pipelines exist (CI, release, dependency-review). Missing onboarding hurts first-run UX. RPM spec at 0.1.0. Error paths not fully hardened. |
| Maintainability | 82/100 | Up from 50/100 — massive improvement. Dead_code annotations reduced from ~4,000 lines of dead asr/ to 24 item-level annotations. Still some cleanup needed. |
| **Production Readiness** | **78/100** | Up from 65/100. Core flow works. The biggest improvement is the removed asr/ dead weight. Remaining gaps: onboarding, error swallowing, frontend test coverage, dead UI components. |

---

## Top Release Blockers

1. **Missing Onboarding Flow** — First-run users see an empty settings window with no guidance. Must be fixed before shipping to non-developer users.

2. **Error Swallowing** — 22 `.catch()` sites silently drop failures. Update failures, startup failures, and recording errors are invisible to users. Must be hardened before release.

---

## High Priority Improvements

3. **Dead UI Components** — Delete 4 unused components (Button, Card, Tabs, Separator) and orphaned CSS. Small effort, immediate bundle size reduction.

4. **Frontend Test Coverage** — Add tests for `pushToTalk.ts` (state machine, error paths, watchdog) and `usePushToTalk.ts` (hotkey events, state sync). These are the most critical untested modules.

5. **Update Error Surfacing** — Propagate update check/download/install failures to `UpdateInfo.errorMessage` so the UI can display them.

6. **`#[allow(dead_code)]` Cleanup** — Convert to `#[expect]` where intentional, gate with `#[cfg(test)]` where possible, remove genuinely unused code.

7. **Remove `SttProviderDiagnostics`** — Dead TypeScript type that's never populated by the backend.

---

## Nice-to-Have Improvements

8. **RPM Spec Version** — Bump from 0.1.0 to match app version. Fix placeholder URL.

9. **Onboard `Contributing.md` Test Instructions** — Document how to run frontend and backend tests separately.

10. **Add `tsc --noEmit` to pre-commit hook** — Catch TypeScript errors earlier in development.

---

## Delta vs Previous Audit

| Metric | Previous | Current | Change |
|--------|----------|---------|--------|
| ASR dead code | ~4,000 lines | 0 lines | **-4,000 lines** |
| `#[allow(dead_code)]` | 3 modules + 7 functions | 1 module + 23 items | **Reduced** |
| `unwrap()` in lib.rs | 2 | 0 | **Fixed** |
| `map_err(\|_\|)` | 8+ locations | 0 | **Fixed** |
| Error swallowing (`.catch(() => {})`) | 8 locations | 5 locations | **Reduced but persists** |
| Runtime artifacts | 6 files | 0 files | **Cleaned up** |
| Dead stores | 1 (`settings.ts`) | 0 | **Fixed** |
| Orphaned docs | 1 (`ASR_ARCHITECTURE.md`) | 0 | **Fixed** |
| New missing features | — | Onboarding flow | **Regressed** |
| Frontend test coverage | Not measured | 13 files vs 30+ source files | **Gap identified** |

### Overall Improvement: **Significant**

The removal of the `asr/` module tree is the single largest positive change. The codebase lost ~40% of its Rust code while gaining clarity and maintainability. Every previous finding was addressed at least partially.

### New Issues Introduced: **Low**

The only genuinely new issue is the missing onboarding flow — but this is more accurately a feature that was specified in AGENTS.md but never implemented, rather than a regression. All other new findings (dead UI components, test coverage gaps, `SttProviderDiagnostics`) existed in the previous audit but were overshadowed by the larger `asr/` problem.

### Architecture Quality Trend: **Strongly Improving**

The architecture is now clean and focused. The pipeline has clear stages with minimal layering. The remaining trait abstractions (CleanupProvider, HotkeyRegistrar) are small and testability-justified.

### Technical Debt Trend: **Declining**

Massive reduction from the previous audit. The remaining debt (24 dead_code annotations, 4 dead UI components, dead types) is trivial compared to the 4,000-line asr/ module that was removed.

### Production Readiness Trend: **Improving**

Up from 65/100 to 78/100. The core pipeline is solid. The remaining gaps are surface-level (onboarding, error handling discipline, test coverage) rather than architectural. None of the remaining issues would block a technical user from successfully using the app.

---

## Final Verdict

### Very Close to Production Ready

The codebase has undergone a major transformation since the previous audit. The single largest architectural liability — the ~4,000-line dead `asr/` abstraction layer — has been completely removed. The recording pipeline, hotkey lifecycle, diagnostics system, and privacy controls are production-quality. The contract mirror discipline between frontend and backend is excellent.

**What stands between this and "Production Ready":**
1. **Onboarding flow** — first-run UX is missing entirely
2. **Error swallowing** — 22 `.catch()` sites need hardening
3. **Frontend test coverage** — critical pipeline orchestration is untested

These are all individually small, well-understood fixes. None require architectural changes. With a focused 2-3 day sprint on these three areas, the project would reach a genuine "Production Ready" state.

The delta from the previous audit is one of the cleanest codebase transformations this reviewer has seen: every actionable finding was addressed, the architecture was flattened, and the team demonstrated the discipline to remove code rather than leave dead scaffolding. The remaining work is finishing-polish, not architecture remediation.

---

*Report generated: June 29, 2026*
*Auditor: principal-engineer audit system*
*Baseline for future audits: YES*
