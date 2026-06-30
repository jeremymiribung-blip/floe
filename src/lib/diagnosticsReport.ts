import type { DiagnosticsReport } from "./tauri";
import { assertNoForbiddenKeys, assertNoForbiddenPatterns } from "./privacy";

export function assertDiagnosticsReportSafe(report: DiagnosticsReport): void {
  assertNoForbiddenKeys(report, "");
  const json = JSON.stringify(report);
  assertNoForbiddenPatterns(json);
}

export function diagnosticsReportToJson(report: DiagnosticsReport): string {
  return JSON.stringify(report, null, 2);
}

export async function copyDiagnosticsReportToClipboard(
  report: DiagnosticsReport,
): Promise<void> {
  assertDiagnosticsReportSafe(report);
  const json = diagnosticsReportToJson(report);
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(json);
    return;
  }
  // Fallback for environments without the async Clipboard API.
  if (typeof document === "undefined") {
    throw new Error("Clipboard is not available in this environment.");
  }
  const textarea = document.createElement("textarea");
  textarea.value = json;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "absolute";
  textarea.style.left = "-9999px";
  document.body.appendChild(textarea);
  textarea.select();
  try {
    const ok = document.execCommand("copy");
    if (!ok) {
      throw new Error("Clipboard copy command failed.");
    }
  } finally {
    document.body.removeChild(textarea);
  }
}

export function emptyDiagnosticsReport(
  app: string,
  app_version: string,
  generated_at: string,
): DiagnosticsReport {
  return {
    schema_version: 1,
    app,
    app_version,
    generated_at,
    environment: "development",
    platform: {
      os: "unknown",
      arch: "unknown",
      family: "unknown",
      tauri_version: null,
      os_version: null,
      cpu_model: null,
      cpu_logical_cores: null,
      memory_total_mb: null,
      memory_available_mb: null,
      process_memory_mb: null,
      uptime_secs: null,
    },
    hotkey: {
      accelerator: "",
      label: "",
      is_default: true,
      is_registered: false,
      error: null,
    },
    settings: {
      api_key_configured: false,
      api_key_masked_preview: null,
      start_at_login_enabled: null,
      start_at_login_available: null,
      keyring_migrated: false,
      feature_flags: {},
    },
    provider_state: {
      configured: false,
      available: false,
    },
    last_session: {
      has_session: false,
      trace_id: null,
      completed: false,
      stage_summary: {},
      stages: {},
      audio: null,
      recovery_actions: [],
      rate_limit: null,
      retries: { stt: 0, cleanup: 0 },
      pipeline_total_ms: null,
      recording_started_at_unix_ms: null,
      recording_ended_at_unix_ms: null,
      detailed_timeline: [],
      cleanup_chars: null,
      cleanup_words: null,
    },
    last_error: null,
    state_flags: {
      api_key_configured: false,
      hotkey_registered: false,
      recording_active: false,
      processing_active: false,
      background_launch: false,
    },
    event_timeline: [],
  };
}
