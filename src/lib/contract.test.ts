import { describe, it, expect } from "vitest";
import * as contract from "./contract";

const c = contract as unknown as Record<string, unknown>;
const COMMAND_KEYS = Object.keys(contract).filter((k) => k.startsWith("CMD_"));
const EVENT_KEYS = Object.keys(contract).filter((k) => k.startsWith("EVENT_"));

const COMMANDS = COMMAND_KEYS.map((k) => c[k] as string);
const EVENTS = EVENT_KEYS.map((k) => c[k] as string);

describe("Contract exports", () => {
  it("all exported values are non-empty strings", () => {
    for (const value of [...COMMANDS, ...EVENTS]) {
      expect(value.length).toBeGreaterThan(0);
    }
  });

  it("all command names are unique", () => {
    expect(new Set(COMMANDS).size).toBe(COMMANDS.length);
  });

  it("all event names are unique", () => {
    expect(new Set(EVENTS).size).toBe(EVENTS.length);
  });

  it("all command names are snake_case", () => {
    for (const cmd of COMMANDS) {
      expect(cmd).toMatch(/^[a-z][a-z0-9_]*$/);
    }
  });

  it("all event names are kebab-case", () => {
    for (const ev of EVENTS) {
      expect(ev).toMatch(/^[a-z][a-z0-9-]*$/);
    }
  });

  it("no command or event name exceeds 64 characters", () => {
    for (const name of [...COMMANDS, ...EVENTS]) {
      expect(name.length).toBeLessThanOrEqual(64);
    }
  });

  it("has at least 33 command constants", () => {
    expect(COMMANDS.length).toBeGreaterThanOrEqual(33);
  });

  it("has at least 5 event constants", () => {
    expect(EVENTS.length).toBeGreaterThanOrEqual(5);
  });

  it("contains required critical commands", () => {
    expect(COMMANDS).toContain(contract.CMD_SAVE_API_KEY);
    expect(COMMANDS).toContain(contract.CMD_GET_API_KEY_STATUS);
    expect(COMMANDS).toContain(contract.CMD_SET_HOTKEY);
    expect(COMMANDS).toContain(contract.CMD_GET_RECORDING_STATUS);
    expect(COMMANDS).toContain(contract.CMD_START_RECORDING);
    expect(COMMANDS).toContain(contract.CMD_STOP_RECORDING);
    expect(COMMANDS).toContain(contract.CMD_TRANSCRIBE_LATEST_RECORDING);
    expect(COMMANDS).toContain(contract.CMD_CLEANUP_TRANSCRIPT);
    expect(COMMANDS).toContain(contract.CMD_COPY_TEXT_TO_CLIPBOARD);
    expect(COMMANDS).toContain(contract.CMD_CHECK_FOR_UPDATE);
  });

  it("contains required events", () => {
    expect(EVENTS).toContain(contract.EVENT_RECORDING_STATE_CHANGED);
    expect(EVENTS).toContain(contract.EVENT_HOTKEY_STATE);
    expect(EVENTS).toContain(contract.EVENT_BUBBLE_STATE);
    expect(EVENTS).toContain(contract.EVENT_SHOW_SETTINGS);
    expect(EVENTS).toContain(contract.EVENT_UPDATE_INSTALLED);
  });

  it("numeric constants are positive", () => {
    expect(contract.MAX_RECORDING_DURATION_SECS).toBeGreaterThan(0);
    expect(contract.TARGET_WAV_SAMPLE_RATE).toBeGreaterThan(0);
    expect(contract.OUTPUT_CHANNELS).toBe(1);
    expect(contract.WAV_BITS_PER_SAMPLE).toBe(16);
  });

  it("BUBBLE_WINDOW_LABEL matches Rust contract", () => {
    expect(contract.BUBBLE_WINDOW_LABEL).toBe("recording-bubble");
  });
});
