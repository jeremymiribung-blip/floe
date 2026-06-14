import { describe, expect, it } from "vitest";
import type { ApiKeyStatus, HotkeyStatus } from "../types/app";
import {
  computeSetupState,
  computeVisibleSetupState,
  isReady,
} from "./setupState";

const apiKeyConfigured: ApiKeyStatus = {
  configured: true,
  maskedPreview: "gsk_...abcd",
};

const apiKeyMissing: ApiKeyStatus = {
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
  it("routes to the API key step when status is unknown", () => {
    expect(computeSetupState(null, null)).toBe("setup_api_key");
  });

  it("routes to the API key step when the key is missing", () => {
    expect(computeSetupState(apiKeyMissing, null)).toBe("setup_api_key");
    expect(computeSetupState(apiKeyMissing, hotkeyRegistered)).toBe(
      "setup_api_key",
    );
  });

  it("routes to the Hotkey step when the key is set but the hotkey is unknown or unregistered", () => {
    expect(computeSetupState(apiKeyConfigured, null)).toBe("setup_hotkey");
    expect(computeSetupState(apiKeyConfigured, hotkeyUnregistered)).toBe(
      "setup_hotkey",
    );
  });

  it("is ready only when the key is configured and the hotkey is registered", () => {
    expect(computeSetupState(apiKeyConfigured, hotkeyRegistered)).toBe("ready");
  });

  it("returns to the API key step after the key is cleared", () => {
    expect(computeSetupState(apiKeyMissing, hotkeyRegistered)).toBe(
      "setup_api_key",
    );
  });

  it("returns to the Hotkey step if the hotkey becomes invalid", () => {
    expect(computeSetupState(apiKeyConfigured, hotkeyUnregistered)).toBe(
      "setup_hotkey",
    );
  });
});

describe("computeVisibleSetupState", () => {
  it("lets returning users skip the Hotkey step when setup is ready", () => {
    expect(
      computeVisibleSetupState(apiKeyConfigured, hotkeyRegistered, false),
    ).toBe("ready");
  });

  it("shows the Hotkey step once after first-time API key setup", () => {
    expect(
      computeVisibleSetupState(apiKeyConfigured, hotkeyRegistered, true),
    ).toBe("setup_hotkey");
  });

  it("keeps unavailable hotkeys on the Hotkey step regardless of the session flag", () => {
    expect(
      computeVisibleSetupState(apiKeyConfigured, hotkeyUnregistered, false),
    ).toBe("setup_hotkey");
  });
});

describe("isReady", () => {
  it("matches the ready state", () => {
    expect(isReady("ready")).toBe(true);
  });

  it("rejects setup states", () => {
    expect(isReady("setup_api_key")).toBe(false);
    expect(isReady("setup_hotkey")).toBe(false);
  });
});
