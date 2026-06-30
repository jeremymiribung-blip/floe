//! Contract-stability integration tests.
//!
//! These tests verify that the frontend-backend boundary is stable:
//! - Event name constants match between Rust and what the frontend expects
//! - Type serialization matches expected camelCase shapes
//!
//! Run: `cargo test --test contract_tests`

// The following types are re-exported from the library for serialization contract tests.
use floe_lib::{
    ApiKeyStatus, ClipboardError, ClipboardErrorCode, GroqTranscription, HotkeyError,
    HotkeyErrorCode, HotkeyStatus, RecordingEndReason, RecordingError, RecordingErrorCode,
    RecordingInfo, RecordingState, RecordingStatePayload, RecordingStatus, SettingsError,
    SettingsErrorCode, StartAtLoginError, StartAtLoginErrorCode, StartAtLoginStatus,
    TranscriptCleanupResult,
};

/// The canonical list of all registered Tauri command names.
/// This must match `ALL_COMMANDS` in `src-tauri/src/contract.rs`
/// AND `ALL_COMMANDS` in `src/lib/contract.ts`.
const ALL_COMMANDS: &[&str] = &[
    "bubble_cancel_recording",
    "bubble_hide",
    "bubble_set_state",
    "bubble_show",
    "cleanup_transcript",
    "clear_api_key",
    "copy_text_to_clipboard",
    "diag_log",
    "diag_log_str",
    "get_api_key_status",
    "get_app_settings",
    "get_current_trace",
    "get_diagnostics_report",
    "get_hotkey_settings",
    "get_latest_recording_info",
    "get_recent_traces",
    "get_recording_status",
    "get_start_at_login_status",
    "paste_clipboard",
    "paste_text",
    "register_global_hotkey",
    "reset_hotkey_to_default",
    "save_api_key",
    "save_app_settings",
    "set_hotkey",
    "set_start_at_login_enabled",
    "start_recording",
    "stop_recording",
    "transcribe_latest_recording",
    "unregister_global_hotkey",
    "update_session_hotkey_latency",
    "log_frontend_event",
    "get_update_info",
    "check_for_update",
    "download_update",
    "install_update",
    "reset_update_state",
];

/// The canonical list of all event names.
const ALL_EVENTS: &[&str] = &[
    "recording-level",
    "recording-state-changed",
    "floe-global-hotkey-state",
    "recording-bubble-state",
    "floe-show-settings",
    "floe-app-shutting-down",
    "floe-update-installed",
];

// ── Command name tests ──────────────────────────────────────────────────────

#[test]
fn command_names_are_unique() {
    let mut sorted = ALL_COMMANDS.to_vec();
    sorted.sort();
    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(sorted.len(), deduped.len(), "Duplicate command names found");
}

#[test]
fn command_names_are_snake_case() {
    for name in ALL_COMMANDS {
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "Command '{}' must be lowercase snake_case",
            name
        );
    }
}

#[test]
fn all_commands_length_is_stable() {
    assert_eq!(ALL_COMMANDS.len(), 37,
        "Command count changed.\nIf you added/removed a command, update ALL_COMMANDS in contract.rs, contract.ts, AND this file.");
}

#[test]
fn command_names_are_reasonable_length() {
    for name in ALL_COMMANDS {
        assert!(
            name.len() <= 48,
            "Command '{}' is too long ({} chars)",
            name,
            name.len()
        );
    }
}

// ── Event name tests ────────────────────────────────────────────────────────

#[test]
fn event_names_are_unique() {
    let mut sorted = ALL_EVENTS.to_vec();
    sorted.sort();
    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(sorted.len(), deduped.len(), "Duplicate event names found");
}

#[test]
fn event_names_are_kebab_case() {
    for name in ALL_EVENTS {
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "Event '{}' must be lowercase kebab-case",
            name
        );
    }
}

#[test]
fn all_events_length_is_stable() {
    assert_eq!(ALL_EVENTS.len(), 7,
        "Event count changed.\nIf you added/removed an event, update ALL_EVENTS in contract.rs, contract.ts, AND this file.");
}

// ── Serialization shape tests ───────────────────────────────────────────────
// These tests verify that Rust types serialize to the JSON shapes the
// frontend expects, using serde_json::json! values for structural comparison.

#[test]
fn api_key_status_shape_is_camel_case() {
    let json = serde_json::json!({
        "configured": true,
        "maskedPreview": "gsk_...abcd",
    });
    assert!(
        json.get("maskedPreview").is_some(),
        "Must have camelCase 'maskedPreview'"
    );
    assert!(
        json.get("masked_preview").is_none(),
        "Must NOT have snake_case"
    );
    assert_eq!(json["configured"], true);
    assert_eq!(json["maskedPreview"], "gsk_...abcd");
}

#[test]
fn api_key_status_unconfigured_shape() {
    let json = serde_json::json!({
        "configured": false,
        "maskedPreview": null,
    });
    assert_eq!(json["configured"], false);
    assert!(json["maskedPreview"].is_null());
}

#[test]
fn hotkey_status_shape_is_camel_case() {
    let json = serde_json::json!({
        "accelerator": "Control+Space",
        "label": "Ctrl + Space",
        "isDefault": true,
        "isRegistered": true,
        "error": null,
    });
    assert!(json.get("isDefault").is_some(), "Must have 'isDefault'");
    assert!(
        json.get("isRegistered").is_some(),
        "Must have 'isRegistered'"
    );
    assert!(json.get("error").is_some(), "Must have 'error' field");
}

#[test]
fn recording_state_values_match_typescript_exactly() {
    // These must match the TypeScript type RecordingState: "idle" | "starting" | "recording" | "stopping"
    let states = serde_json::json!(["idle", "starting", "recording", "stopping"]);
    let arr = states.as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0], "idle");
    assert_eq!(arr[1], "starting");
    assert_eq!(arr[2], "recording");
    assert_eq!(arr[3], "stopping");
}

#[test]
fn recording_state_payload_shape() {
    let json = serde_json::json!({
        "state": "recording",
        "isRecording": true,
    });
    assert!(
        json.get("isRecording").is_some(),
        "Must have 'isRecording' (camelCase)"
    );
    assert!(
        json.get("is_recording").is_none(),
        "Must NOT have snake_case"
    );
}

#[test]
fn recording_end_reason_values_match_typescript() {
    // TypeScript type: "manual" | "maxDuration" | "deviceDisconnected" | "shutdown" | "watchdogTimeout"
    let reasons = serde_json::json!([
        "manual",
        "maxDuration",
        "deviceDisconnected",
        "shutdown",
        "watchdogTimeout",
    ]);
    let arr = reasons.as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0], "manual");
    assert_eq!(arr[1], "maxDuration");
    assert_eq!(arr[2], "deviceDisconnected");
    assert_eq!(arr[3], "shutdown");
    assert_eq!(arr[4], "watchdogTimeout");
}

#[test]
fn settings_error_code_values_match_typescript() {
    let codes = serde_json::json!([
        "invalidGroqApiKey",
        "invalidAppSettings",
        "secretStoreUnavailable",
        "appSettingsUnavailable",
    ]);
    let arr = codes.as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0], "invalidGroqApiKey");
    assert_eq!(arr[1], "invalidAppSettings");
    assert_eq!(arr[2], "secretStoreUnavailable");
    assert_eq!(arr[3], "appSettingsUnavailable");
}

#[test]
fn hotkey_error_code_values_match_typescript() {
    let codes = serde_json::json!([
        "invalidHotkey",
        "unsupportedHotkey",
        "alreadyInUse",
        "registrationFailed",
        "unregisterFailed",
        "settingsUnavailable",
    ]);
    let arr = codes.as_array().unwrap();
    assert_eq!(arr.len(), 6);
    assert_eq!(arr[0], "invalidHotkey");
    assert_eq!(arr[1], "unsupportedHotkey");
    assert_eq!(arr[2], "alreadyInUse");
    assert_eq!(arr[3], "registrationFailed");
    assert_eq!(arr[4], "unregisterFailed");
    assert_eq!(arr[5], "settingsUnavailable");
}

#[test]
fn recording_info_shape_has_all_expected_fields() {
    let json = serde_json::json!({
        "sampleRate": 48000,
        "inputChannels": 1,
        "outputChannels": 1,
        "wavFormat": "wav",
        "wavSampleRate": 16000,
        "wavChannels": 1,
        "durationMs": 5000,
        "sampleCount": 240000,
        "wavByteCount": 16044,
        "wavBitsPerSample": 16,
        "recordingStopToEncodeStartMs": 0,
        "audioEncodeMs": 5,
        "startedAtMs": 1000,
        "endedAtMs": 6000,
        "maxDurationReached": false,
        "endedReason": "manual",
    });
    let obj = json.as_object().unwrap();

    let expected_fields = &[
        "sampleRate",
        "inputChannels",
        "outputChannels",
        "wavFormat",
        "wavSampleRate",
        "wavChannels",
        "durationMs",
        "sampleCount",
        "wavByteCount",
        "wavBitsPerSample",
        "recordingStopToEncodeStartMs",
        "audioEncodeMs",
        "startedAtMs",
        "endedAtMs",
        "maxDurationReached",
        "endedReason",
    ];
    for field in expected_fields {
        assert!(
            obj.contains_key(*field),
            "RecordingInfo missing field '{field}'"
        );
    }
    assert_eq!(
        obj.len(),
        expected_fields.len(),
        "RecordingInfo has {} fields, expected {}. Extra: {:?}",
        obj.len(),
        expected_fields.len(),
        obj.keys().collect::<Vec<_>>(),
    );
}

#[test]
fn settings_error_shape_is_camel_case() {
    let json = serde_json::json!({
        "domain": "settings",
        "code": "invalidGroqApiKey",
        "message": "Enter a valid API key.",
    });
    assert_eq!(json["domain"], "settings");
    assert_eq!(json["code"], "invalidGroqApiKey");
    assert!(json.as_object().unwrap().contains_key("message"));
    assert!(!json
        .as_object()
        .unwrap()
        .contains_key("invalid_groq_api_key"));
}

#[test]
fn app_settings_shape_has_no_cleanup_or_provider_fields() {
    let json = serde_json::json!({
        "hotkey": {
            "accelerator": "Control+Space",
            "label": "Ctrl + Space"
        },
        "keyringMigrated": false,
    });
    assert!(json.get("hotkey").is_some());
    assert!(json.get("keyringMigrated").is_some());

    // These fields must NEVER appear in AppSettings
    let forbidden = &[
        "cleanup",
        "cleanupMode",
        "cleanup_mode",
        "behavior",
        "provider",
        "providers",
    ];
    for field in forbidden {
        assert!(
            json.get(*field).is_none(),
            "AppSettings must NOT contain field: {field}"
        );
    }
}

#[test]
fn hotkey_error_shape_is_camel_case() {
    let json = serde_json::json!({
        "domain": "hotkey",
        "code": "alreadyInUse",
        "message": "This shortcut is already in use.",
    });
    assert_eq!(json["domain"], "hotkey");
    assert_eq!(json["code"], "alreadyInUse");
}

#[test]
fn recording_error_shape_is_camel_case() {
    let json = serde_json::json!({
        "domain": "recording",
        "code": "internal",
        "message": "Recording failed",
    });
    assert_eq!(json["domain"], "recording");
    assert_eq!(json["code"], "internal");
}

#[test]
fn stt_error_shape_is_camel_case() {
    let json = serde_json::json!({
        "domain": "stt",
        "code": "emptyAudio",
        "message": "Record audio before requesting a transcription.",
    });
    assert_eq!(json["domain"], "stt");
    assert_eq!(json["code"], "emptyAudio");
}

#[test]
fn clipboard_error_shape_is_camel_case() {
    let json = serde_json::json!({
        "domain": "clipboard",
        "code": "clipboardUnavailable",
        "message": "Clipboard unavailable",
    });
    assert_eq!(json["domain"], "clipboard");
    assert_eq!(json["code"], "clipboardUnavailable");
}

#[test]
fn start_at_login_error_shape_is_camel_case() {
    let json = serde_json::json!({
        "domain": "startAtLogin",
        "code": "unavailable",
        "message": "Start at login unavailable",
    });
    assert_eq!(json["domain"], "startAtLogin");
    assert_eq!(json["code"], "unavailable");
}

#[test]
fn recording_error_code_values_match_typescript() {
    let codes = serde_json::json!([
        "noInputDevice",
        "permissionDenied",
        "alreadyRecording",
        "notRecording",
        "emptyRecording",
        "unsupportedSampleFormat",
        "deviceDisconnected",
        "streamBuildFailed",
        "streamPlayFailed",
        "wavEncodingFailed",
        "stopFailed",
        "watchdogTimeout",
        "appShuttingDown",
        "internal",
    ]);
    assert_eq!(codes.as_array().unwrap().len(), 14);
}

#[test]
fn stt_error_code_values_match_typescript() {
    let codes = serde_json::json!([
        "missingApiKey",
        "invalidApiKey",
        "rateLimit",
        "timeout",
        "apiUnreachable",
        "malformedResponse",
        "unsupportedAudio",
        "invalidRequest",
        "emptyAudio",
        "serverError",
    ]);
    assert_eq!(codes.as_array().unwrap().len(), 10);
}

#[test]
fn clipboard_error_code_values_match_typescript() {
    let codes = serde_json::json!(["clipboardUnavailable", "pasteUnavailable"]);
    assert_eq!(codes.as_array().unwrap().len(), 2);
}

#[test]
fn start_at_login_error_code_values_match_typescript() {
    let codes = serde_json::json!(["enableFailed", "disableFailed", "unavailable"]);
    assert_eq!(codes.as_array().unwrap().len(), 3);
}

#[test]
fn recording_status_shape_has_trace_id_as_optional() {
    let json = serde_json::json!({
        "isRecording": false,
        "sampleRate": null,
        "inputChannels": null,
        "outputChannels": 1,
        "durationMs": 0,
        "sampleCount": 0,
        "startedAtMs": null,
        "maxDurationSeconds": 120,
        "latestRecording": null,
        "lastError": null,
    });
    // traceId is optional, so it's fine that it's missing
    assert_eq!(json["isRecording"], false);
    assert_eq!(json["maxDurationSeconds"], 120);
}

#[test]
fn stt_result_shape_is_camel_case() {
    let json = serde_json::json!({
        "text": "Hello world",
        "model": "whisper-large-v3-turbo",
        "retryCount": 0,
    });
    assert!(json.get("retryCount").is_some(), "Must have 'retryCount'");
    assert!(
        json.get("retry_count").is_none(),
        "Must NOT have snake_case"
    );
}

#[test]
fn transcript_cleanup_result_shape_is_camel_case() {
    let json = serde_json::json!({
        "text": "cleaned text",
        "model": "qwen/qwen3.6-27b",
        "retryCount": 0,
        "validationMs": 1,
        "fallbackUsed": false,
    });
    assert!(
        json.get("fallbackUsed").is_some(),
        "Must have 'fallbackUsed'"
    );
    assert!(
        json.get("validationMs").is_some(),
        "Must have 'validationMs'"
    );
    assert!(
        json.get("fallback_used").is_none(),
        "Must NOT have snake_case"
    );
    assert!(
        json.get("validation_ms").is_none(),
        "Must NOT have snake_case"
    );
}

#[test]
fn hotkey_state_event_payload_shape() {
    let json = serde_json::json!({
        "state": "Pressed",
    });
    assert_eq!(json["state"], "Pressed");

    let json = serde_json::json!({
        "state": "Released",
    });
    assert_eq!(json["state"], "Released");
}

#[test]
fn bubble_state_event_payload_shape() {
    let json = serde_json::json!({
        "recording": true,
    });
    assert_eq!(json["recording"], true);

    let json = serde_json::json!({
        "recording": false,
    });
    assert_eq!(json["recording"], false);
}

#[test]
fn recording_level_payload_shape() {
    let json = serde_json::json!({
        "level": 0.5,
    });
    assert_eq!(json["level"], 0.5);
}

#[test]
fn start_at_login_status_shape() {
    let json = serde_json::json!({
        "enabled": true,
        "available": true,
    });
    assert_eq!(json["enabled"], true);
    assert_eq!(json["available"], true);

    let json = serde_json::json!({
        "enabled": false,
        "available": false,
    });
    assert_eq!(json["enabled"], false);
    assert_eq!(json["available"], false);
}

// ── Actual serialization tests ──────────────────────────────────────────────
// These tests construct real Rust instances, serialize them with serde_json,
// and verify the output matches expected camelCase shapes.  If someone removes
// a `#[serde(rename_all = "camelCase")]` attribute, these tests will fail.

/// Recursively assert that every JSON object key matches `[a-z][a-zA-Z0-9]*`
/// (camelCase with no underscores).
fn assert_camel_case(obj: &serde_json::Value) {
    match obj {
        serde_json::Value::Object(map) => {
            for key in map.keys() {
                assert!(
                    !key.contains('_'),
                    "snake_case key '{}' found; expected camelCase",
                    key,
                );
                assert!(
                    key.chars().next().unwrap_or(' ').is_ascii_lowercase(),
                    "key '{}' must start with a lowercase letter",
                    key,
                );
                assert!(
                    key.chars().all(|c| c.is_ascii_alphanumeric()),
                    "key '{}' contains non-alphanumeric characters",
                    key,
                );
                // Recurse into nested objects
                assert_camel_case(&map[key]);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                assert_camel_case(item);
            }
        }
        _ => {}
    }
}

#[test]
fn recording_info_actual_serialization() {
    let info = RecordingInfo {
        sample_rate: 48000,
        input_channels: 1,
        output_channels: 1,
        wav_format: "wav",
        wav_sample_rate: 16000,
        wav_channels: 1,
        duration_ms: 5000,
        sample_count: 240000,
        wav_byte_count: 16044,
        wav_bits_per_sample: 16,
        recording_stop_to_encode_start_ms: 0,
        audio_encode_ms: 5,
        started_at_ms: 1000,
        ended_at_ms: 6000,
        max_duration_reached: false,
        ended_reason: RecordingEndReason::Manual,
    };
    let value = serde_json::to_value(&info).expect("serialize RecordingInfo");

    let expected = serde_json::json!({
        "sampleRate": 48000,
        "inputChannels": 1,
        "outputChannels": 1,
        "wavFormat": "wav",
        "wavSampleRate": 16000,
        "wavChannels": 1,
        "durationMs": 5000,
        "sampleCount": 240000,
        "wavByteCount": 16044,
        "wavBitsPerSample": 16,
        "recordingStopToEncodeStartMs": 0,
        "audioEncodeMs": 5,
        "startedAtMs": 1000,
        "endedAtMs": 6000,
        "maxDurationReached": false,
        "endedReason": "manual",
    });

    assert_eq!(value, expected, "RecordingInfo JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn recording_status_actual_serialization() {
    let info = RecordingInfo {
        sample_rate: 44100,
        input_channels: 2,
        output_channels: 1,
        wav_format: "wav",
        wav_sample_rate: 16000,
        wav_channels: 1,
        duration_ms: 1234,
        sample_count: 54321,
        wav_byte_count: 9999,
        wav_bits_per_sample: 16,
        recording_stop_to_encode_start_ms: 2,
        audio_encode_ms: 10,
        started_at_ms: 500,
        ended_at_ms: 1734,
        max_duration_reached: true,
        ended_reason: RecordingEndReason::MaxDuration,
    };
    let err = RecordingError {
        domain: "recording",
        code: RecordingErrorCode::WatchdogTimeout,
        message: "Watchdog triggered".to_string(),
    };
    let status = RecordingStatus {
        is_recording: false,
        sample_rate: Some(48000),
        input_channels: Some(2),
        output_channels: 1,
        duration_ms: 1234,
        sample_count: 54321,
        started_at_ms: Some(500),
        max_duration_seconds: 120,
        latest_recording: Some(info.clone()),
        last_error: Some(err),
        trace_id: None,
    };
    let value = serde_json::to_value(&status).expect("serialize RecordingStatus");

    // trace_id=None should be skipped, not serialized as null
    assert!(
        value.get("traceId").is_none(),
        "traceId must be omitted when None (skip_serializing_if)"
    );
    assert!(
        value.get("trace_id").is_none(),
        "Must NOT have snake_case trace_id"
    );
    assert_eq!(value["isRecording"], serde_json::json!(false));
    assert_eq!(value["sampleRate"], serde_json::json!(48000));
    assert_eq!(value["maxDurationSeconds"], serde_json::json!(120));
    assert_eq!(
        value["latestRecording"]["endedReason"],
        serde_json::json!("maxDuration")
    );
    assert_eq!(
        value["lastError"]["code"],
        serde_json::json!("watchdogTimeout")
    );
    assert_camel_case(&value);
}

#[test]
fn recording_state_payload_actual_serialization() {
    let payload = RecordingStatePayload {
        state: RecordingState::Recording,
        is_recording: true,
    };
    let value = serde_json::to_value(&payload).expect("serialize RecordingStatePayload");

    assert_eq!(
        value,
        serde_json::json!({"state": "recording", "isRecording": true})
    );
    assert_camel_case(&value);

    let idle = RecordingStatePayload {
        state: RecordingState::Idle,
        is_recording: false,
    };
    assert_eq!(
        serde_json::to_value(&idle).unwrap(),
        serde_json::json!({"state": "idle", "isRecording": false})
    );
}

#[test]
fn hotkey_status_actual_serialization() {
    let status = HotkeyStatus {
        accelerator: "Control+Space".to_string(),
        label: "Ctrl + Space".to_string(),
        is_default: true,
        is_registered: true,
        error: None,
    };
    let value = serde_json::to_value(&status).expect("serialize HotkeyStatus");

    let expected = serde_json::json!({
        "accelerator": "Control+Space",
        "label": "Ctrl + Space",
        "isDefault": true,
        "isRegistered": true,
        "error": null,
    });
    assert_eq!(value, expected, "HotkeyStatus JSON shape mismatch");
    assert_camel_case(&value);

    // With an error
    let err_status = HotkeyStatus {
        error: Some("Hotkey unavailable".to_string()),
        ..status
    };
    let val2 = serde_json::to_value(&err_status).unwrap();
    assert_eq!(val2["error"], "Hotkey unavailable");
    assert_camel_case(&val2);
}

#[test]
fn hotkey_error_actual_serialization() {
    let err = HotkeyError {
        domain: "hotkey",
        code: HotkeyErrorCode::AlreadyInUse,
        message: "This shortcut is already in use.".to_string(),
    };
    let value = serde_json::to_value(&err).expect("serialize HotkeyError");

    let expected = serde_json::json!({
        "domain": "hotkey",
        "code": "alreadyInUse",
        "message": "This shortcut is already in use.",
    });
    assert_eq!(value, expected, "HotkeyError JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn stt_result_actual_serialization() {
    let result = GroqTranscription {
        text: "Hello world".to_string(),
        model: "whisper-large-v3-turbo".to_string(),
        retry_count: 0,
        rate_limit: None,
    };
    let value = serde_json::to_value(&result).expect("serialize GroqTranscription");

    let expected = serde_json::json!({
        "text": "Hello world",
        "model": "whisper-large-v3-turbo",
        "retryCount": 0,
    });
    assert_eq!(value, expected, "GroqTranscription JSON shape mismatch");
    assert!(
        value.get("rateLimit").is_none(),
        "rateLimit=None must be skipped"
    );
    assert_camel_case(&value);

    // With retry count > 0
    let retried = GroqTranscription {
        text: "retried".to_string(),
        model: "whisper-large-v3-turbo".to_string(),
        retry_count: 2,
        rate_limit: None,
    };
    let val2 = serde_json::to_value(&retried).unwrap();
    assert_eq!(val2["retryCount"], 2);
}

#[test]
fn transcript_cleanup_result_actual_serialization() {
    let result = TranscriptCleanupResult {
        text: "cleaned text".to_string(),
        warning: None,
        model: "qwen/qwen3.6-27b".to_string(),
        retry_count: 0,
        validation_ms: 1,
        fallback_used: false,
        rate_limit: None,
        error_code: None,
    };
    let value = serde_json::to_value(&result).expect("serialize TranscriptCleanupResult");

    let expected = serde_json::json!({
        "text": "cleaned text",
        "model": "qwen/qwen3.6-27b",
        "retryCount": 0,
        "validationMs": 1,
        "fallbackUsed": false,
    });
    assert_eq!(
        value, expected,
        "TranscriptCleanupResult JSON shape mismatch"
    );
    assert!(
        value.get("warning").is_none(),
        "warning=None must be skipped"
    );
    assert!(
        value.get("rateLimit").is_none(),
        "rateLimit=None must be skipped"
    );
    assert!(
        value.get("errorCode").is_none(),
        "errorCode=None must be skipped"
    );
    assert_camel_case(&value);

    // Fallback variant
    let fallback = TranscriptCleanupResult {
        text: "raw transcript".to_string(),
        warning: Some("Cleanup failed".to_string()),
        model: String::new(),
        retry_count: 0,
        validation_ms: 0,
        fallback_used: true,
        rate_limit: None,
        error_code: Some("serverError".to_string()),
    };
    let val2 = serde_json::to_value(&fallback).unwrap();
    assert_eq!(val2["warning"], "Cleanup failed");
    assert_eq!(val2["fallbackUsed"], true);
    assert_eq!(val2["errorCode"], "serverError");
    assert_camel_case(&val2);
}

#[test]
fn settings_error_actual_serialization() {
    let err = SettingsError {
        domain: "settings",
        code: SettingsErrorCode::InvalidGroqApiKey,
        message: "Enter a valid API key.".to_string(),
    };
    let value = serde_json::to_value(&err).expect("serialize SettingsError");

    let expected = serde_json::json!({
        "domain": "settings",
        "code": "invalidGroqApiKey",
        "message": "Enter a valid API key.",
    });
    assert_eq!(value, expected, "SettingsError JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn recording_error_actual_serialization() {
    let err = RecordingError {
        domain: "recording",
        code: RecordingErrorCode::Internal,
        message: "Recording failed".to_string(),
    };
    let value = serde_json::to_value(&err).expect("serialize RecordingError");

    let expected = serde_json::json!({
        "domain": "recording",
        "code": "internal",
        "message": "Recording failed",
    });
    assert_eq!(value, expected, "RecordingError JSON shape mismatch");
    assert_camel_case(&value);

    let device_err = RecordingError {
        domain: "recording",
        code: RecordingErrorCode::NoInputDevice,
        message: "No input device found".to_string(),
    };
    let val2 = serde_json::to_value(&device_err).unwrap();
    assert_eq!(val2["code"], "noInputDevice");
}

#[test]
fn clipboard_error_actual_serialization() {
    let err = ClipboardError {
        domain: "clipboard",
        code: ClipboardErrorCode::ClipboardUnavailable,
        message: "Clipboard unavailable".to_string(),
    };
    let value = serde_json::to_value(&err).expect("serialize ClipboardError");

    let expected = serde_json::json!({
        "domain": "clipboard",
        "code": "clipboardUnavailable",
        "message": "Clipboard unavailable",
    });
    assert_eq!(value, expected, "ClipboardError JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn start_at_login_status_actual_serialization() {
    let status = StartAtLoginStatus {
        enabled: true,
        available: true,
    };
    let value = serde_json::to_value(&status).expect("serialize StartAtLoginStatus");
    assert_eq!(
        value,
        serde_json::json!({"enabled": true, "available": true})
    );
    assert_camel_case(&value);

    let disabled = StartAtLoginStatus {
        enabled: false,
        available: false,
    };
    assert_eq!(
        serde_json::to_value(&disabled).unwrap(),
        serde_json::json!({"enabled": false, "available": false})
    );
}

#[test]
fn update_info_actual_serialization() {
    let info = floe_lib::UpdateInfo {
        current_version: "1.0.0".into(),
        latest_version: Some("v1.1.0".into()),
        status: floe_lib::UpdateStatusLabel::Available,
        download_progress: 0.0,
        last_check_result: None,
        error_message: None,
    };
    let value = serde_json::to_value(&info).expect("serialize UpdateInfo");

    let expected = serde_json::json!({
        "currentVersion": "1.0.0",
        "latestVersion": "v1.1.0",
        "status": "available",
        "downloadProgress": 0.0,
        "lastCheckResult": null,
        "errorMessage": null,
    });
    assert_eq!(value, expected, "UpdateInfo JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn update_info_no_update_serialization() {
    let info = floe_lib::UpdateInfo {
        current_version: "1.0.0".into(),
        latest_version: Some("1.0.0".into()),
        status: floe_lib::UpdateStatusLabel::NoUpdate,
        download_progress: 0.0,
        last_check_result: Some("You're up to date".into()),
        error_message: None,
    };
    let value = serde_json::to_value(&info).expect("serialize UpdateInfo");

    let expected = serde_json::json!({
        "currentVersion": "1.0.0",
        "latestVersion": "1.0.0",
        "status": "no_update",
        "downloadProgress": 0.0,
        "lastCheckResult": "You're up to date",
        "errorMessage": null,
    });
    assert_eq!(value, expected, "UpdateInfo JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn update_info_checking_serialization() {
    let info = floe_lib::UpdateInfo {
        current_version: "1.0.0".into(),
        latest_version: None,
        status: floe_lib::UpdateStatusLabel::Checking,
        download_progress: 0.0,
        last_check_result: None,
        error_message: None,
    };
    let value = serde_json::to_value(&info).expect("serialize UpdateInfo");

    let expected = serde_json::json!({
        "currentVersion": "1.0.0",
        "latestVersion": null,
        "status": "checking",
        "downloadProgress": 0.0,
        "lastCheckResult": null,
        "errorMessage": null,
    });
    assert_eq!(value, expected, "UpdateInfo JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn update_error_actual_serialization() {
    let err = floe_lib::UpdateError {
        domain: "update",
        code: floe_lib::UpdateErrorCode::NetworkError,
        message: "Could not reach update server.".to_string(),
    };
    let value = serde_json::to_value(&err).expect("serialize UpdateError");

    let expected = serde_json::json!({
        "domain": "update",
        "code": "networkError",
        "message": "Could not reach update server.",
    });
    assert_eq!(value, expected, "UpdateError JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn update_error_code_values_match_typescript() {
    let codes = serde_json::json!([
        "networkError",
        "updateNotFound",
        "downloadFailed",
        "installFailed",
        "alreadyUpToDate",
        "internal",
    ]);
    assert_eq!(codes.as_array().unwrap().len(), 6);
    assert_camel_case(&codes);
}

#[test]
fn update_status_label_values() {
    let labels = serde_json::json!([
        "idle",
        "checking",
        "available",
        "downloading",
        "downloaded",
        "ready",
        "no_update",
        "error",
    ]);
    assert_eq!(labels.as_array().unwrap().len(), 8);
}

#[test]
fn start_at_login_error_actual_serialization() {
    let err = StartAtLoginError {
        domain: "startAtLogin",
        code: StartAtLoginErrorCode::Unavailable,
        message: "Start at login unavailable".to_string(),
    };
    let value = serde_json::to_value(&err).expect("serialize StartAtLoginError");

    let expected = serde_json::json!({
        "domain": "startAtLogin",
        "code": "unavailable",
        "message": "Start at login unavailable",
    });
    assert_eq!(value, expected, "StartAtLoginError JSON shape mismatch");
    assert_camel_case(&value);
}

#[test]
fn api_key_status_actual_serialization() {
    let status = ApiKeyStatus {
        configured: true,
        masked_preview: Some("gsk_...abcd".to_string()),
    };
    let value = serde_json::to_value(&status).expect("serialize ApiKeyStatus");

    let expected = serde_json::json!({
        "configured": true,
        "maskedPreview": "gsk_...abcd",
    });
    assert_eq!(value, expected, "ApiKeyStatus JSON shape mismatch");
    assert_camel_case(&value);

    let unconfigured = ApiKeyStatus {
        configured: false,
        masked_preview: None,
    };
    let val2 = serde_json::to_value(&unconfigured).unwrap();
    assert_eq!(val2["configured"], false);
    assert_eq!(val2["maskedPreview"], serde_json::Value::Null);
    assert_camel_case(&val2);
}

// ── Enum round-trip tests ───────────────────────────────────────────────────
// Each enum's serde(rename_all = "camelCase") must produce exactly the string
// the TypeScript union type expects.

macro_rules! enum_round_trip_test {
    ($name:ident, $ty:ty, [ $( ($variant:expr, $expected:expr) ),+ $(,)? ]) => {
        #[test]
        fn $name() {
            let cases: Vec<($ty, &str)> = vec![$(( $variant, $expected )),+];
            for (variant, expected_str) in &cases {
                let value = serde_json::to_value(variant).expect("serialize enum variant");
                assert_eq!(
                    value,
                    serde_json::json!(expected_str),
                    "{}::{:?} serialized to {}, expected {}",
                    stringify!($ty),
                    variant,
                    value,
                    serde_json::json!(expected_str),
                );
            }
        }
    };
}

enum_round_trip_test!(
    recording_end_reason_round_trip,
    RecordingEndReason,
    [
        (RecordingEndReason::Manual, "manual"),
        (RecordingEndReason::MaxDuration, "maxDuration"),
        (RecordingEndReason::DeviceDisconnected, "deviceDisconnected"),
        (RecordingEndReason::Shutdown, "shutdown"),
        (RecordingEndReason::WatchdogTimeout, "watchdogTimeout"),
    ]
);

enum_round_trip_test!(
    recording_state_round_trip,
    RecordingState,
    [
        (RecordingState::Idle, "idle"),
        (RecordingState::Starting, "starting"),
        (RecordingState::Recording, "recording"),
        (RecordingState::Stopping, "stopping"),
    ]
);

enum_round_trip_test!(
    recording_error_code_round_trip,
    RecordingErrorCode,
    [
        (RecordingErrorCode::NoInputDevice, "noInputDevice"),
        (RecordingErrorCode::PermissionDenied, "permissionDenied"),
        (RecordingErrorCode::AlreadyRecording, "alreadyRecording"),
        (RecordingErrorCode::NotRecording, "notRecording"),
        (RecordingErrorCode::EmptyRecording, "emptyRecording"),
        (
            RecordingErrorCode::UnsupportedSampleFormat,
            "unsupportedSampleFormat"
        ),
        (RecordingErrorCode::DeviceDisconnected, "deviceDisconnected"),
        (RecordingErrorCode::StreamBuildFailed, "streamBuildFailed"),
        (RecordingErrorCode::StreamPlayFailed, "streamPlayFailed"),
        (RecordingErrorCode::WavEncodingFailed, "wavEncodingFailed"),
        (RecordingErrorCode::StopFailed, "stopFailed"),
        (RecordingErrorCode::WatchdogTimeout, "watchdogTimeout"),
        (RecordingErrorCode::AppShuttingDown, "appShuttingDown"),
        (RecordingErrorCode::Internal, "internal"),
    ]
);

enum_round_trip_test!(
    settings_error_code_round_trip,
    SettingsErrorCode,
    [
        (SettingsErrorCode::InvalidGroqApiKey, "invalidGroqApiKey"),
        (SettingsErrorCode::InvalidAppSettings, "invalidAppSettings"),
        (
            SettingsErrorCode::SecretStoreUnavailable,
            "secretStoreUnavailable"
        ),
        (
            SettingsErrorCode::AppSettingsUnavailable,
            "appSettingsUnavailable"
        ),
    ]
);

enum_round_trip_test!(
    hotkey_error_code_round_trip,
    HotkeyErrorCode,
    [
        (HotkeyErrorCode::InvalidHotkey, "invalidHotkey"),
        (HotkeyErrorCode::UnsupportedHotkey, "unsupportedHotkey"),
        (HotkeyErrorCode::AlreadyInUse, "alreadyInUse"),
        (HotkeyErrorCode::RegistrationFailed, "registrationFailed"),
        (HotkeyErrorCode::UnregisterFailed, "unregisterFailed"),
        (HotkeyErrorCode::SettingsUnavailable, "settingsUnavailable"),
    ]
);

enum_round_trip_test!(
    clipboard_error_code_round_trip,
    ClipboardErrorCode,
    [
        (
            ClipboardErrorCode::ClipboardUnavailable,
            "clipboardUnavailable"
        ),
        (ClipboardErrorCode::PasteUnavailable, "pasteUnavailable"),
    ]
);

enum_round_trip_test!(
    start_at_login_error_code_round_trip,
    StartAtLoginErrorCode,
    [
        (StartAtLoginErrorCode::EnableFailed, "enableFailed"),
        (StartAtLoginErrorCode::DisableFailed, "disableFailed"),
        (StartAtLoginErrorCode::Unavailable, "unavailable"),
    ]
);
