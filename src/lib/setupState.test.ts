import { describe, expect, it } from "vitest";
import type { GroqApiKeyStatus, HotkeyStatus } from "../types/app";
import {
  computeSetupState,
  computeVisibleSetupState,
  isReady,
} from "./setupState";

const groqConfigured: GroqApiKeyStatus = {
  configured: true,
  maskedPreview: "gsk_...abcd",
};

const groqMissing: GroqApiKeyStatus = {
  configured: false,
  maskedPreview: null,
};

const hotkeyRegistered: HotkeyStatus = {
  accelerator: "Control+Space",
  label: "Ctrl + Space",
  isDefault: true,
  isRegistered: true,
  error: null,
};

const hotkeyUnregistered: HotkeyStatus = {
  accelerator: "Control+Space",
  label: "Ctrl + Space",
  isDefault: true,
  isRegistered: false,
  error: "Hotkey unavailable",
};

describe("computeSetupState", () => {
  it("routes to the Groq step when status is unknown", () => {
    expect(computeSetupState(null, null)).toBe("setup_groq");
  });

  it("routes to the Groq step when the key is missing", () => {
    expect(computeSetupState(groqMissing, null)).toBe("setup_groq");
    expect(computeSetupState(groqMissing, hotkeyRegistered)).toBe("setup_groq");
  });

  it("routes to the Hotkey step when the key is set but the hotkey is unknown or unregistered", () => {
    expect(computeSetupState(groqConfigured, null)).toBe("setup_hotkey");
    expect(computeSetupState(groqConfigured, hotkeyUnregistered)).toBe(
      "setup_hotkey",
    );
  });

  it("is ready only when the key is configured and the hotkey is registered", () => {
    expect(computeSetupState(groqConfigured, hotkeyRegistered)).toBe("ready");
  });

  it("returns to the Groq step after the key is cleared", () => {
    expect(computeSetupState(groqMissing, hotkeyRegistered)).toBe("setup_groq");
  });

  it("returns to the Hotkey step if the hotkey becomes invalid", () => {
    expect(computeSetupState(groqConfigured, hotkeyUnregistered)).toBe(
      "setup_hotkey",
    );
  });
});

describe("computeVisibleSetupState", () => {
  it("lets returning users skip the Hotkey step when setup is ready", () => {
    expect(
      computeVisibleSetupState(groqConfigured, hotkeyRegistered, false),
    ).toBe("ready");
  });

  it("shows the Hotkey step once after first-time Groq setup", () => {
    expect(
      computeVisibleSetupState(groqConfigured, hotkeyRegistered, true),
    ).toBe("setup_hotkey");
  });

  it("keeps unavailable hotkeys on the Hotkey step regardless of the session flag", () => {
    expect(
      computeVisibleSetupState(groqConfigured, hotkeyUnregistered, false),
    ).toBe("setup_hotkey");
  });
});

describe("isReady", () => {
  it("matches the ready state", () => {
    expect(isReady("ready")).toBe(true);
  });

  it("rejects setup states", () => {
    expect(isReady("setup_groq")).toBe(false);
    expect(isReady("setup_hotkey")).toBe(false);
  });
});
