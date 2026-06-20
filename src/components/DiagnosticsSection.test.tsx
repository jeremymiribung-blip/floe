import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  fireEvent,
  waitFor,
} from "@testing-library/react";
import { DiagnosticsSection } from "./DiagnosticsSection";

vi.mock("../lib/tauri", () => ({
  isTauriRuntime: () => true,
  getDiagnosticsReport: vi.fn(() =>
    Promise.resolve({
      schema_version: 1,
      app: "Floe",
      app_version: "0.1.0",
      generated_at: "2026-06-18T00:00:00.000Z",
      platform: {
        os: "macos",
        arch: "aarch64",
        family: "unix",
        tauri_version: null,
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
      },
      last_session: {
        has_session: true,
        trace_id: "deadbeef",
        completed: true,
        stage_summary: {},
        stages: {},
        audio: null,
        stt_provider: null,
        recovery_actions: [],
        rate_limit: null,
        retries: { stt: 0, cleanup: 0 },
        pipeline_total_ms: 2_737,
        recording_started_at_ms: null,
        recording_ended_at_ms: null,
      },
      last_error: null,
      state_flags: {
        api_key_configured: true,
        hotkey_registered: true,
        recording_active: false,
        processing_active: false,
        background_launch: false,
      },
      event_timeline: [],
    }),
  ),
}));

beforeEach(() => {
  cleanup();
});

afterEach(() => {
  cleanup();
});

describe("DiagnosticsSection", () => {
  it("renders a copy button and a refresh button", async () => {
    render(<DiagnosticsSection appVersion="0.1.0" />);
    expect(
      await screen.findByRole("button", { name: /copy diagnostics json/i }),
    ).toBeDefined();
    expect(
      await screen.findByRole("button", { name: /refresh diagnostics/i }),
    ).toBeDefined();
  });

  it("toggles the JSON preview when the preview header is clicked", async () => {
    render(<DiagnosticsSection appVersion="0.1.0" />);
    const toggle = await screen.findByRole("button", { name: /preview json/i });
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(screen.getByText(/"schema_version": 1/)).toBeDefined();
    });
  });

  it("copies JSON via the clipboard API when the copy button is clicked", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<DiagnosticsSection appVersion="0.1.0" />);
    const copy = await screen.findByRole("button", {
      name: /copy diagnostics json/i,
    });
    fireEvent.click(copy);
    await waitFor(() => {
      expect(writeText).toHaveBeenCalled();
    });
  });
});
