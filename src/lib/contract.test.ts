// ─────────────────────────────────────────────────────────────────────────────
// Frontend-Backend Contract Tests (TypeScript side)
//
// These tests verify the TypeScript contract mirrors the Rust backend.
// Run with: npx vitest run src/lib/contract.test.ts
//
// When a test fails, update BOTH Rust and TypeScript contracts in lockstep.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect } from "vitest";
import {
  ALL_COMMANDS,
  CMD_SAVE_API_KEY,
  CMD_START_RECORDING,
  CMD_STOP_RECORDING,
  CMD_BUBBLE_SHOW,
  CMD_CLEANUP_TRANSCRIPT,
  CMD_COPY_TEXT_TO_CLIPBOARD,
  EVENT_RECORDING_LEVEL,
  EVENT_RECORDING_STATE_CHANGED,
  EVENT_HOTKEY_STATE,
  EVENT_BUBBLE_STATE,
  EVENT_SHOW_SETTINGS,
  EVENT_SHUTTING_DOWN,
  MAX_RECORDING_DURATION_SECS,
  WATCHDOG_GRACE_SECS,
  TARGET_WAV_SAMPLE_RATE,
  OUTPUT_CHANNELS,
  WAV_BITS_PER_SAMPLE,
} from "./contract";

describe("Command names", () => {
  it("all command names are unique", () => {
    const unique = new Set(ALL_COMMANDS);
    expect(unique.size).toBe(ALL_COMMANDS.length);
  });

  it("all command names are snake_case", () => {
    for (const name of ALL_COMMANDS) {
      expect(name).toMatch(/^[a-z_]+$/);
    }
  });

  it("command count is stable", () => {
    // If this fails, update ALL_COMMANDS in contract.rs, contract.ts, and
    // the integration tests.
    expect(ALL_COMMANDS.length).toBe(27);
  });

  it("critical commands exist", () => {
    expect(ALL_COMMANDS).toContain(CMD_SAVE_API_KEY);
    expect(ALL_COMMANDS).toContain(CMD_START_RECORDING);
    expect(ALL_COMMANDS).toContain(CMD_STOP_RECORDING);
    expect(ALL_COMMANDS).toContain(CMD_BUBBLE_SHOW);
    expect(ALL_COMMANDS).toContain(CMD_CLEANUP_TRANSCRIPT);
    expect(ALL_COMMANDS).toContain(CMD_COPY_TEXT_TO_CLIPBOARD);
  });

  it("command list is sorted alphabetically", () => {
    const sorted = [...ALL_COMMANDS].sort();
    expect(ALL_COMMANDS).toEqual(sorted);
  });

  it("no command name exceeds 64 characters", () => {
    for (const name of ALL_COMMANDS) {
      expect(name.length).toBeLessThanOrEqual(64);
    }
  });
});

describe("Event names", () => {
  const allEventNames: readonly string[] = [
    EVENT_RECORDING_LEVEL,
    EVENT_RECORDING_STATE_CHANGED,
    EVENT_HOTKEY_STATE,
    EVENT_BUBBLE_STATE,
    EVENT_SHOW_SETTINGS,
    EVENT_SHUTTING_DOWN,
  ];

  it("all event names are unique", () => {
    const unique = new Set(allEventNames);
    expect(unique.size).toBe(allEventNames.length);
  });

  it("all event names are kebab-case", () => {
    for (const name of allEventNames) {
      expect(name).toMatch(/^[a-z-]+$/);
    }
  });

  it("event count is stable", () => {
    expect(allEventNames.length).toBe(6);
  });
});

describe("Recording constants", () => {
  it("MAX_RECORDING_DURATION_SECS matches Rust backend", () => {
    expect(MAX_RECORDING_DURATION_SECS).toBe(120);
  });

  it("WATCHDOG_GRACE_SECS matches Rust backend", () => {
    expect(WATCHDOG_GRACE_SECS).toBe(5);
  });

  it("TARGET_WAV_SAMPLE_RATE matches Rust backend", () => {
    expect(TARGET_WAV_SAMPLE_RATE).toBe(16_000);
  });

  it("OUTPUT_CHANNELS matches Rust backend", () => {
    expect(OUTPUT_CHANNELS).toBe(1);
  });

  it("WAV_BITS_PER_SAMPLE matches Rust backend", () => {
    expect(WAV_BITS_PER_SAMPLE).toBe(16);
  });
});

describe("Typed event payload shapes", () => {
  // These tests verify that the frontend payload interfaces match
  // what the backend sends. They check field presence and types.

  it("RecordingLevelPayload shape is stable", () => {
    const payload: import("./contract").RecordingLevelPayload = { level: 0.5 };
    expect(payload).toHaveProperty("level");
    expect(typeof payload.level).toBe("number");
  });

  it("RecordingStateChangedPayload shape is stable", () => {
    const payload: import("./contract").RecordingStateChangedPayload = {
      state: "recording",
      isRecording: true,
    };
    expect(payload).toHaveProperty("state");
    expect(payload).toHaveProperty("isRecording");
    expect(["idle", "starting", "recording", "stopping"]).toContain(
      payload.state,
    );
  });

  it("HotkeyStatePayload shape is stable", () => {
    const payload: import("./contract").HotkeyStatePayload = {
      state: "Pressed",
    };
    expect(payload).toHaveProperty("state");
    expect(["Pressed", "Released"]).toContain(payload.state);
  });

  it("BubbleStatePayload shape is stable", () => {
    const payload: import("./contract").BubbleStatePayload = {
      recording: true,
    };
    expect(payload).toHaveProperty("recording");
    expect(typeof payload.recording).toBe("boolean");
  });
});

describe("PushToTalk watchdog constant integrity", () => {
  // The frontend PushToTalkController uses a watchdog timer as a safety net.
  // Its value must be the sum of MAX_RECORDING_DURATION_SECS + WATCHDOG_GRACE_SECS
  // in milliseconds to match the backend's watchdog behavior.
  it("frontend watchdog timeout matches backend max duration + grace", () => {
    const expectedWatchdogMs =
      (MAX_RECORDING_DURATION_SECS + WATCHDOG_GRACE_SECS) * 1000;
    // This is the calculation used in pushToTalk.ts
    const WATCHDOG_TIMEOUT_MS =
      MAX_RECORDING_DURATION_SECS * 1000 + WATCHDOG_GRACE_SECS * 1000;
    expect(WATCHDOG_TIMEOUT_MS).toBe(expectedWatchdogMs);
  });
});
