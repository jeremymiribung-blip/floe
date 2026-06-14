# Floe ASR Architecture Documentation

## Overview

Floe implements a **provider-agnostic ASR (Automatic Speech Recognition) architecture** that supports multiple speech-to-text providers through a common interface. This architecture is designed for:

- **Flexibility**: Add new ASR providers without changing core application logic
- **Resilience**: Automatic fallback to alternative providers when primary fails
- **Privacy**: Strict controls on what data is logged or exposed
- **Resource Management**: Enforcement of limits on audio duration, size, and concurrent operations

## Core Principles

From AGENTS.md:

> - Respect the STT rule: one Groq STT request after recording stops.
> - Do not add streaming, chunking, transcript merging, or realtime partials.
> - Floe uses Groq for STT.
> - Floe uses Groq Whisper Turbo (`whisper-large-v3-turbo`) for STT.
> - Floe sends optimized 16 kHz mono 16-bit PCM WAV to Groq after recording stops.
> - The same Groq API key handles both STT and cleanup; it is stored under `groq-api-key` in the OS keychain.
> - Audio is never sent for cleanup. Only transcript text is sent for cleanup.
> - If cleanup fails, Floe falls back to pasting the raw Groq transcript and surfaces a `Cleanup failed` warning.

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                         Application Layer                          │
│  (commands, settings, recording, bubble, clipboard, cleanup)       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                           ASR Backend                               │
│  (asr::backend::AsrBackend)                                      │
│  - Coordinates provider selection                                │
│  - Enforces resource policies                                    │
│  - Manages fallback strategy                                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         Provider Registry                          │
│  (asr::registry::ProviderRegistry)                               │
│  - Registers and manages all ASR providers                       │
│  - Implements selection logic with criteria                       │
│  - Maintains health status cache                                 │
│  - Tracks disabled/experimental providers                       │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────────┼───────────────────┐
              ▼                   ▼                   ▼
┌──────────────────┐ ┌──────────────────┐ ┌──────────────────┐
│   Groq Adapter    │ │   Vosk Adapter    │ │ Whisper Local     │
│  (asr::adapters:: │ │  (asr::adapters:: │ │  Adapter          │
│     groq::*)       │ │    vosk::*)       │ │ (asr::adapters:: │
└──────────────────┘ └──────────────────┘ │   whisper_local)   │
                                              └──────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                          ASR Runtime                                │
│  (asr::runtime::*) - Internal streaming infrastructure           │
│  - NOT exposed to UI                                                 │
│  - Streaming happens internally in background                    │
│  - Only final transcript sent to cleanup/paste                      │
└─────────────────────────────────────────────────────────────────┘
```

## Provider Interface

All ASR providers implement the `AsrProvider` trait:

```rust
pub trait AsrProvider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    fn default_model(&self) -> &'static str;
    fn available_models(&self) -> &[ModelSpec];

    async fn create_session(&self, config: SessionConfig)
        -> Result<Box<dyn AsrSession>, SessionError>;

    async fn health_check(&self) -> Result<HealthStatus, ()>;
}
```

### Provider Capabilities

Each provider declares its capabilities:

```rust
pub struct ProviderCapabilities {
    pub backend_type: BackendType,    // Native, Onnx, Cloud
    pub deployment: Deployment,        // Local, Cloud
    pub streaming: StreamingSupport,    // None, Full
    pub partials: bool,               // Supports partial results
    pub timestamps: bool,             // Supports word timestamps
    pub gpu_required: bool,           // Requires GPU
    pub fallback_compatible: bool,    // Can be used as fallback
    pub max_audio_seconds: u64,
    pub supported_sample_rates: Vec<u32>,
    pub min_audio_bytes: u64,
    pub max_audio_bytes: u64,
}
```

## Current Providers

### 1. Groq Adapter (`asr::adapters::groq`)

**First Adapter** - The primary, always-available cloud provider.

- **ID**: `groq`
- **Name**: Groq Cloud
- **Backend**: Cloud
- **Deployment**: Cloud
- **Default Model**: `whisper-large-v3-turbo`
- **Fallback Compatible**: ✅ Yes
- **GPU Required**: ❌ No
- **Streaming Support**: Full

**Behavior**:

- Uses Groq's OpenAI-compatible API endpoint
- Sends 16 kHz mono 16-bit PCM WAV audio
- Single request after recording stops (per AGENTS.md rule)
- API key from OS keychain (`groq-api-key`)

### 2. Vosk Adapter (`asr::adapters::vosk`)

**Second Adapter** - Local ONNX-based provider.

- **ID**: `vosk`
- **Name**: Vosk Local
- **Backend**: ONNX
- **Deployment**: Local
- **Default Model**: `vosk-model-en-us-0.22`
- **Fallback Compatible**: ❌ No (by design)
- **GPU Required**: ❌ No
- **Streaming Support**: Full

**Behavior**:

- Uses Kaldi-based models in ONNX format
- Models must be downloaded and placed in standard locations
- Supports multiple model sizes and languages
- Feature flag controlled (`FLOE_VOSK_MODEL_PATH` env var)

### 3. Whisper Local Adapter (`asr::adapters::whisper_local`)

- **ID**: `whisper_local`
- **Name**: Whisper Local
- **Backend**: Native
- **Deployment**: Local
- **Default Model**: `base`
- **Fallback Compatible**: ✅ Yes
- **GPU Required**: Depends on model

**Models Available**:

- `tiny` (39M params, no GPU)
- `base` (74M params, no GPU) - **Default**
- `small` (244M params, no GPU)
- `medium` (769M params, GPU required)
- `large-v2` (1.5B params, GPU required)

## Provider Selection

The `ProviderRegistry` selects providers based on `SelectionCriteria`:

```rust
pub struct SelectionCriteria {
    pub preferred: Option<String>,          // User's preferred provider
    pub audio_duration_ms: u64,           // Duration for policy checks
    pub requires_fallback_compatible: bool, // Must support fallback
    pub requires_local: bool,              // Must be local deployment
    pub requires_streaming: bool,           // Must support streaming
}
```

### Selection Algorithm

1. If `preferred` is specified and available, use it
2. Otherwise, try the default provider if it matches criteria
3. Otherwise, iterate through all registered providers and return the first match
4. If no match, return `SelectionError::NoSuitableProvider`

### Fallback Provider

The registry provides a dedicated `fallback_provider()` method:

1. **Priority**: Groq is always preferred as fallback if available
2. **Fallback Chain**: If Groq is disabled, use any other fallback-compatible provider
3. **None**: Returns `None` if no fallback-compatible provider exists

## Fallback Strategy

The `FallbackStrategy` (`asr::fallback`) implements:

1. **Primary Attempt**: Try the selected provider first
2. **Retry on Retryable Errors**: If error is retryable, retry up to 2 times with exponential backoff (250ms → 500ms → 1000ms)
3. **Fallback on Non-Retryable Errors**: If error is NOT retryable, immediately try fallback
4. **Fallback Execution**: If fallback provider exists, attempt transcription
5. **Error Propagation**: If both fail, return combined error message

### Error Classification

All transcription errors are classified:

```rust
pub enum TranscriptionErrorCode {
    Timeout,          // Retryable
    ApiUnreachable,   // Retryable
    RateLimit,        // Retryable
    ServerError,      // Retryable
    InvalidAuth,      // NOT retryable (bad API key)
    InvalidRequest,   // NOT retryable
    UnsupportedAudio, // NOT retryable
    MalformedResponse,// NOT retryable
    Internal,         // Depends on context
}
```

### Retry Behavior

- **Max Attempts**: 2 retries (total of 3 attempts including initial)
- **Backoff**: Exponential starting at 250ms, doubling each retry
- **Jitter**: No (deterministic for testing)

## Resource Policy

The `ResourcePolicy` (`asr::policy`) enforces limits:

```rust
pub struct ResourcePolicy {
    pub max_concurrent_sessions: usize,    // Default: 1
    pub session_timeout_secs: u64,        // Default: 60
    pub gpu_memory_limit_mb: Option<u64>, // Default: None
    pub allow_local_models: bool,           // Default: false
    pub allow_streaming: bool,             // Default: false
    pub max_audio_duration_secs: u64,     // Default: 120
    pub max_audio_bytes: u64,              // Default: 25,000,000
}
```

### Policy Enforcement

Audio is validated before transcription:

- Duration must be ≤ `max_audio_duration_secs`
- Size must be ≤ `max_audio_bytes`
- Violations return `AsrErrorCode::AudioTooLong` or `AsrErrorCode::AudioTooLarge`

## Streaming Architecture

**Key Principle from AGENTS.md**:

> Streaming is active internally in the background. Only final transcript continues to cleanup/paste.

### Internal Streaming (Runtime Layer)

The `asr::runtime` module provides:

- **Chunking**: Audio is processed in chunks (default: 320ms)
- **Partial Results**: Available internally but NOT exposed to UI
- **Session Management**: Long-lived sessions for streaming
- **Backpressure**: Flow control when runtime is busy

**Important**: Despite internal streaming, from the user's perspective:

- One STT request after recording stops
- Only final transcript is used
- No partial results shown to user
- No realtime merging or chunking visible

### Runtime Process

The runtime layer (`asr::runtime::supervisor`) manages:

- **Process Lifecycle**: Start, stop, restart, monitor
- **Heartbeat**: Regular liveness checks (default: every 5 seconds)
- **Timeout**: Heartbeat timeout (default: 15 seconds)
- **Crash Detection**: Monitors process health, tracks consecutive crashes
- **Auto-Restart**: Configurable automatic recovery (default: disabled)

**Process States**:

```rust
pub enum ProcessState {
    Stopped,      // Not running
    Starting,     // Initializing
    Running,      // Operational and healthy
    ShuttingDown, // Graceful shutdown in progress
    Crashed,      // Unexpected termination
    Error,        // Error state (not crashed but not healthy)
}
```

### Crash Recovery

- **Max Consecutive Crashes**: 5 (default)
- **After Max**: Process stays in Crashed state, manual intervention required
- **Restart Delay**: Configurable with exponential backoff

## Diagnostics and Privacy

### Diagnostics Structure

`AsrDiagnostics` captures transcription metadata (NOT the actual transcript):

```rust
pub struct AsrDiagnostics {
    pub trace_version: u8,              // For forward compatibility
    pub created_at: String,             // ISO 8601 timestamp
    pub platform: String,              // OS platform
    pub provider_name: String,         // Provider ID
    pub model_name: String,            // Model ID
    pub backend_type: BackendType,     // Native/Onnx/Cloud
    pub audio_duration_ms: u64,
    pub transcription_ms: u64,
    pub cleanup_ms: u64,
    pub realtime_factor: f64,          // transcription_ms / audio_duration_ms
    pub fallback_used: bool,
    pub fallback_provider: Option<String>,
    pub retry_count: u32,
    pub error_code: Option<String>,    // SANITIZED
}
```

### Privacy Rules

**NEVER LOGGED**:

- Raw transcript text
- Raw audio data
- Full API keys
- Auth headers
- Clipboard contents
- HTTP request/response bodies

**ALWAYS SANITIZED**:

- Error codes (removes secrets, normalizes characters)
- Any user-provided strings in diagnostics

### Error Code Sanitization

The `sanitize_error_code()` function (in `asr::types`):

1. **Redacts**: Any string containing `bearer`, `authorization`, `api_key`, `gsk_`
2. **Normalizes**: Lowercase, replace special chars with `_`
3. **Limits**: Truncates to 64 characters, replaces empty with `internal`

### DiagLog

The `commands::diag::DiagLog` provides privacy-safe logging:

- **Type-Level Safety**: `DiagEntry` struct only has approved fields
- **No Text Field**: Cannot accidentally log transcript
- **Sanitized Errors**: Error codes are sanitized before logging
- **Optional Path**: Only writes if path is configured

**Example Log Entry**:

```
[08:30:45.123] provider_name=groq,model_name=whisper-large-v3-turbo,backend_type=cloud,audio_duration_ms=5000,transcription_ms=1200,cleanup_ms=300,realtime_factor=0.240,retry_count=0,fallback_used=false,error_code=timeout_error
```

## Model Loading and Cache Behavior

### Model Manager

The `ModelManager` (`asr::model`) manages model metadata and overrides:

```rust
pub struct ModelManager {
    models: HashMap<&'static str, Vec<ModelSpec>>,
    overrides: HashMap<String, String>,
}
```

**Features**:

- Register models for each provider
- Get default model for provider
- Find specific model
- Override default model selection
- List all available model IDs

### Model Loading (Local Providers)

Local providers (Vosk, Whisper Local) load models on-demand:

1. **Model Path Resolution**:
   - Environment variable (e.g., `FLOE_VOSK_MODEL_PATH`)
   - Next to executable
   - In `vosk-models/` or `whisper-models/` subdirectory
   - User's home directory (`.floe/vosk-models/`)
   - System-wide location (`/usr/share/floe/`)

2. **Validation**:
   - Check required files exist
   - Validate model compatibility
   - Verify model is not corrupted

3. **Caching**:
   - Models are loaded once and reused
   - No automatic eviction (manual unload required)
   - Cache is per-process (not persisted)

### Resource Policy for Model Loading

- **GPU Memory Limit**: If set, prevents loading models exceeding limit
- **Local Models Flag**: Must be enabled to allow local model loading
- **Max Sessions**: Limits concurrent model usage

## How to Enable Providers

### Groq (Always Enabled)

Groq is always registered as the primary provider:

```rust
// In lib.rs setup
let mut registry = asr::registry::ProviderRegistry::new();
let _ = registry.register(Box::new(asr::adapters::groq::GroqAdapter::new(
    groq_http_client,
    api_key,
)));
```

**Requirements**:

- Valid Groq API key in OS keychain (`groq-api-key`)
- Internet connectivity to Groq API

### Vosk (Feature Flag)

Vosk is conditionally registered:

```rust
if asr::adapters::vosk::is_vosk_enabled() {
    let _ = registry.register(Box::new(
        asr::adapters::vosk::VoskAdapter::with_default_model(),
    ));
}
```

**Enable**: Set `FLOE_VOSK_ENABLED=true` environment variable OR have models installed.

**Requirements**:

- Vosk ONNX models downloaded and in path
- No API key required (local inference)

### Whisper Local (Feature Flag)

Whisper Local is conditionally registered:

```rust
if asr::adapters::whisper_local::is_local_whisper_enabled() {
    let _ = registry.register(Box::new(
        asr::adapters::whisper_local::WhisperLocalAdapter::with_default_model(),
    ));
}
```

**Enable**: Set `FLOE_WHISPER_ENABLED=true` environment variable OR have models installed.

**Requirements**:

- Whisper ONNX models downloaded and in path
- No API key required (local inference)
- GPU required for larger models

## How to Disable Providers

### At Registration Time

```rust
let mut registry = asr::registry::ProviderRegistry::new();
registry.register(Box::new(GroqAdapter::new(...))).unwrap();
registry.mark_disabled("groq");  // Disable Groq
```

### Via Settings

Providers can be disabled through the `SettingsManager`:

- Disabled providers are persisted in settings
- Re-enabled through settings UI or API

## What is NOT Implemented Yet

**Intentional Gaps** (from AGENTS.md constraints):

1. **No Real-Time Streaming to UI**:
   - Streaming is internal only
   - No partial results shown to user
   - No chunk merging
   - Only final transcript after recording stops

2. **No Provider Switching for Cleanup**:
   - Cleanup always uses Groq
   - No fallback cleanup providers
   - No cleanup mode selection

3. **No Behavior Settings**:
   - No configuration options for STT behavior
   - No quality/accuracy tradeoffs
   - No language selection

4. **No Cerebras Support**:
   - Only Groq for cloud STT
   - No multi-cloud support

5. **No Qwen or GPT-OSS Cleanup**:
   - Cleanup uses Groq Llama 3.3 70B Versatile only
   - No alternative cleanup models

6. **No Runtime Process in Production Yet**:
   - Runtime layer exists but may not be fully integrated
   - Streaming happens in-process currently
   - Runtime process for background streaming is scaffolded but not required

## Risk Assessment

### Low Risk

- ✅ Provider selection logic is well-tested
- ✅ Fallback strategy is deterministic
- ✅ Privacy protections are type-level and runtime
- ✅ Resource policy enforcement is straightforward
- ✅ Groq as default is stable and proven

### Medium Risk

- ⚠️ Local provider model loading may have edge cases
- ⚠️ Runtime process management complexity
- ⚠️ Cross-platform path resolution for local models

### High Risk

- ❌ None identified in current architecture

## Testing Coverage

Comprehensive tests exist for:

1. **Provider Selection**: `asr/tests/provider_selection.rs`
   - Default selection
   - Preferred provider
   - Criteria-based selection
   - Disabled providers
   - Fallback provider selection

2. **Adapter Tests**: `asr/tests/adapter_tests.rs`
   - Groq adapter identity and capabilities
   - Vosk adapter identity and capabilities
   - Whisper Local adapter identity and capabilities
   - Cross-adapter uniqueness

3. **Fallback Tests**: `asr/tests/fallback_tests.rs`
   - Primary success (no fallback)
   - Non-retryable failure triggers fallback
   - Retryable failure retries then falls back
   - No fallback returns error
   - Audio preservation during fallback
   - Error message propagation

4. **Runtime Tests**: `asr/tests/runtime_tests.rs`
   - Process state transitions
   - Heartbeat tracking
   - Crash counting
   - Configuration defaults

5. **Policy Tests**: `asr/tests/policy_tests.rs`
   - Audio validation (duration and size)
   - Policy configuration
   - Custom limits

6. **Diagnostics Tests**: `asr/tests/diagnostics_tests.rs`
   - Diagnostics construction
   - Realtime factor calculation
   - Error code sanitization
   - Chained modifications

7. **Privacy Tests**: `asr/tests/privacy_tests.rs`
   - No sensitive fields in DiagEntry
   - Error code redaction
   - Safe logging to files
   - Type-level privacy enforcement

## File References

- **Core ASR**: `src-tauri/src/asr/`
- **Adapters**: `src-tauri/src/asr/adapters/`
- **Runtime**: `src-tauri/src/asr/runtime/`
- **Commands**: `src-tauri/src/commands/`
- **Tests**: `src-tauri/src/asr/tests/`

## Summary

The Floe ASR architecture is **provider-neutral by design**:

1. All providers implement the same `AsrProvider` trait
2. Selection is criteria-based and configurable
3. Fallback to Groq is always available (if not disabled)
4. Streaming is internal and invisible to users
5. Privacy is enforced at type level and through sanitization
6. Resource limits prevent abuse

The architecture supports the AGENTS.md constraints:

- One Groq STT request after recording stops ✅
- No streaming/chunking/partials exposed to UI ✅
- Groq fallback always available ✅
- Privacy protections in place ✅
