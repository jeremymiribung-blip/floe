import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  assertDiagnosticsReportSafe,
  copyDiagnosticsReportToClipboard,
  diagnosticsReportToJson,
  emptyDiagnosticsReport,
} from "./diagnosticsReport";
import { getDiagnosticsReport, type DiagnosticsReport } from "./tauri";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

afterEach(() => {
  vi.clearAllMocks();
});

function sampleReport(): DiagnosticsReport {
  return {
    schema_version: 1,
    app: "Floe",
    app_version: "1.0.0",
    generated_at: "2026-06-18T00:00:00.000Z",
    environment: "development",
    platform: {
      os: "macos",
      arch: "aarch64",
      family: "unix",
      tauri_version: null,
      os_version: "macOS 15.0",
      cpu_model: "Apple M1",
      cpu_logical_cores: 8,
      memory_total_mb: 8192,
      memory_available_mb: 4096,
      process_memory_mb: 128,
      uptime_secs: 3600,
    },
    hotkey: {
      accelerator: "Alt+Space",
      label: "Option + Space",
      is_default: true,
      is_registered: true,
      error: null,
    },
    settings: {
      api_key_configured: true,
      api_key_masked_preview: "gsk_…****",
      start_at_login_enabled: false,
      start_at_login_available: true,
      keyring_migrated: true,
      feature_flags: {},
    },
    provider_state: {
      configured: true,
      available: true,
    },
    last_session: {
      has_session: true,
      trace_id: "deadbeef",
      completed: true,
      stage_summary: {
        hotkey_ok: true,
        recording_ok: true,
        transcription_ok: true,
        cleanup_ok: true,
        cleanup_fallback_used: false,
        clipboard_ok: true,
        paste_ok: true,
        copied_only: false,
        error_stage: null,
        sanitized_error_code: null,
      },
      stages: {
        hotkey_to_recording_start: {
          status: "succeeded",
          duration_ms: 25,
          attempts: 1,
          model: null,
          error_code: null,
          skipped_reason: null,
        },
        audio_capture: {
          status: "succeeded",
          duration_ms: 1_500,
          attempts: 1,
          model: null,
          error_code: null,
          skipped_reason: null,
        },
        transcription: {
          status: "succeeded",
          duration_ms: 800,
          attempts: 1,
          model: "whisper-large-v3-turbo",
          error_code: null,
          skipped_reason: null,
        },
        cleanup: {
          status: "succeeded",
          duration_ms: 300,
          attempts: 1,
          model: "llama-3.3-70b-versatile",
          error_code: null,
          skipped_reason: null,
        },
        clipboard_write: {
          status: "succeeded",
          duration_ms: 8,
          attempts: 1,
          model: null,
          error_code: null,
          skipped_reason: null,
        },
        paste: {
          status: "succeeded",
          duration_ms: 80,
          attempts: 1,
          model: null,
          error_code: null,
          skipped_reason: null,
        },
      },
      audio: {
        format: "wav",
        sample_rate: 16_000,
        channels: 1,
        bits_per_sample: 16,
        bytes: 32_000,
        duration_ms: 1_000,
        ended_reason: "manual",
        max_duration_reached: false,
      },
      stt_provider: {
        provider_name: "groq",
        model: "whisper-large-v3-turbo",
        audio_duration_ms: 1_000,
        transcription_ms: 800,
        realtime_factor: 0.8,
        fallback_used: false,
      },
      recovery_actions: [],
      rate_limit: null,
      retries: { stt: 0, cleanup: 0 },
      pipeline_total_ms: 2_737,
      recording_started_at_unix_ms: 1_000,
      recording_ended_at_unix_ms: 2_000,
      detailed_timeline: [
        {
          stage: "hotkey_to_recording_start",
          status: "succeeded",
          duration_ms: 25,
        },
      ],
      cleanup_chars: null,
      cleanup_words: null,
    },
    last_error: null,
    state_flags: {
      api_key_configured: true,
      hotkey_registered: true,
      recording_active: false,
      processing_active: false,
      background_launch: false,
    },
    event_timeline: [
      {
        stage: "hotkey_to_recording_start",
        status: "succeeded",
        duration_ms: 25,
        attempts: 1,
        error_code: null,
      },
      {
        stage: "transcription",
        status: "succeeded",
        duration_ms: 800,
        attempts: 1,
        error_code: null,
      },
    ],
  };
}

describe("diagnosticsReport", () => {
  describe("getDiagnosticsReport", () => {
    it("invokes the get_diagnostics_report command", async () => {
      const expected = sampleReport();
      mockedInvoke.mockResolvedValue(expected);
      const result = await getDiagnosticsReport();
      expect(mockedInvoke).toHaveBeenCalledWith("get_diagnostics_report");
      expect(result).toEqual(expected);
    });
  });

  describe("diagnosticsReportToJson", () => {
    it("produces valid JSON with snake_case keys", () => {
      const json = diagnosticsReportToJson(sampleReport());
      const parsed = JSON.parse(json);
      expect(parsed.schema_version).toBe(1);
      expect(parsed.app).toBe("Floe");
      expect(parsed.app_version).toBe("1.0.0");
      expect(parsed.hotkey.is_default).toBe(true);
      expect(parsed.last_session.stages.transcription.model).toBe(
        "whisper-large-v3-turbo",
      );
      expect(parsed.last_session.pipeline_total_ms).toBe(2_737);
      expect(Array.isArray(parsed.event_timeline)).toBe(true);
    });
  });

  describe("assertDiagnosticsReportSafe", () => {
    it("accepts a clean report", () => {
      expect(() => assertDiagnosticsReportSafe(sampleReport())).not.toThrow();
    });

    it("rejects reports with bearer token strings", () => {
      const dirty: DiagnosticsReport = {
        ...sampleReport(),
        last_error: {
          stage: "stt",
          code: "bearer gsk_secret123",
          message: "Authorization header was malformed",
        },
      };
      expect(() => assertDiagnosticsReportSafe(dirty)).toThrow(
        /forbidden pattern/i,
      );
    });

    it("rejects reports with raw groq keys", () => {
      const dirty: DiagnosticsReport = {
        ...sampleReport(),
        settings: {
          ...sampleReport().settings,
          api_key_masked_preview: "gsk_abc123def456ghi789",
        },
      };
      expect(() => assertDiagnosticsReportSafe(dirty)).toThrow(
        /forbidden pattern/i,
      );
    });

    it("rejects reports with forbidden keys", () => {
      const dirty = JSON.parse(
        JSON.stringify(sampleReport()),
      ) as DiagnosticsReport;
      (dirty.last_session as unknown as Record<string, unknown>).transcript =
        "leaked transcript";
      expect(() => assertDiagnosticsReportSafe(dirty)).toThrow(
        /forbidden key/i,
      );
    });
  });

  describe("copyDiagnosticsReportToClipboard", () => {
    let originalClipboard: PropertyDescriptor | undefined;
    let originalExecCommand: ((command: string) => boolean) | undefined;

    beforeEach(() => {
      originalClipboard = Object.getOwnPropertyDescriptor(
        navigator,
        "clipboard",
      );
      originalExecCommand = document.execCommand as
        | ((command: string) => boolean)
        | undefined;
    });

    afterEach(() => {
      if (originalClipboard) {
        Object.defineProperty(navigator, "clipboard", originalClipboard);
      } else {
        delete (navigator as unknown as Record<string, unknown>).clipboard;
      }
      if (originalExecCommand) {
        document.execCommand = originalExecCommand;
      }
    });

    it("uses the async clipboard API when available", async () => {
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText },
      });

      await copyDiagnosticsReportToClipboard(sampleReport());

      expect(writeText).toHaveBeenCalledTimes(1);
      const written = writeText.mock.calls[0][0];
      expect(typeof written).toBe("string");
      expect(() => JSON.parse(written)).not.toThrow();
    });

    it("falls back to execCommand when clipboard API is unavailable", async () => {
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: undefined,
      });
      document.execCommand = vi.fn(() => true);

      await expect(
        copyDiagnosticsReportToClipboard(sampleReport()),
      ).resolves.toBeUndefined();
    });

    it("rejects with an error when copy fails", async () => {
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: undefined,
      });
      document.execCommand = vi.fn(() => false);

      await expect(
        copyDiagnosticsReportToClipboard(sampleReport()),
      ).rejects.toThrow(/copy/i);
    });
  });

  describe("emptyDiagnosticsReport", () => {
    it("returns a safe-to-share empty report with snake_case keys", () => {
      const report = emptyDiagnosticsReport(
        "Floe",
        "1.0.0",
        "2026-01-01T00:00:00.000Z",
      );
      const parsed = JSON.parse(diagnosticsReportToJson(report));
      expect(parsed.schema_version).toBe(1);
      expect(parsed.app).toBe("Floe");
      expect(parsed.last_session.has_session).toBe(false);
      expect(parsed.last_session.trace_id).toBeNull();
      expect(parsed.last_session.stages).toEqual({});
      expect(parsed.event_timeline).toEqual([]);
      expect(() => assertDiagnosticsReportSafe(report)).not.toThrow();
    });
  });
});
