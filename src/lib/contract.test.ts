import { describe, it, expect } from "vitest";
import {
  CMD_SAVE_API_KEY,
  CMD_CLEAR_API_KEY,
  CMD_GET_API_KEY_STATUS,
  CMD_GET_HOTKEY_SETTINGS,
  CMD_SET_HOTKEY,
  CMD_RESET_HOTKEY_TO_DEFAULT,
  CMD_GET_START_AT_LOGIN_STATUS,
  CMD_SET_START_AT_LOGIN_ENABLED,
  CMD_GET_RECORDING_STATUS,
  EVENT_BUBBLE_STATE,
  EVENT_SHOW_SETTINGS,
} from "./contract";

const COMMANDS = [
  CMD_SAVE_API_KEY,
  CMD_CLEAR_API_KEY,
  CMD_GET_API_KEY_STATUS,
  CMD_GET_HOTKEY_SETTINGS,
  CMD_SET_HOTKEY,
  CMD_RESET_HOTKEY_TO_DEFAULT,
  CMD_GET_START_AT_LOGIN_STATUS,
  CMD_SET_START_AT_LOGIN_ENABLED,
  CMD_GET_RECORDING_STATUS,
] as const;

const EVENTS = [EVENT_BUBBLE_STATE, EVENT_SHOW_SETTINGS] as const;

describe("Command names", () => {
  it("all command names are unique", () => {
    const names = [...COMMANDS];
    expect(new Set(names).size).toBe(names.length);
  });

  it("all command names are snake_case", () => {
    for (const cmd of COMMANDS) {
      expect(cmd).toMatch(/^[a-z][a-z0-9_]*$/);
    }
  });

  it("count is stable at 9", () => {
    expect(COMMANDS.length).toBe(9);
  });

  it("critical commands exist", () => {
    expect(COMMANDS).toContain(CMD_SAVE_API_KEY);
    expect(COMMANDS).toContain(CMD_GET_API_KEY_STATUS);
    expect(COMMANDS).toContain(CMD_SET_HOTKEY);
    expect(COMMANDS).toContain(CMD_GET_RECORDING_STATUS);
  });

  it("no command name exceeds 64 characters", () => {
    for (const cmd of COMMANDS) {
      expect(cmd.length).toBeLessThanOrEqual(64);
    }
  });
});

describe("Event names", () => {
  it("all event names are unique", () => {
    const names = [...EVENTS];
    expect(new Set(names).size).toBe(names.length);
  });

  it("count is stable at 2", () => {
    expect(EVENTS.length).toBe(2);
  });

  it("all event names are non-empty strings", () => {
    for (const ev of EVENTS) {
      expect(typeof ev).toBe("string");
      expect(ev.length).toBeGreaterThan(0);
    }
  });
});
