import type { ApiKeyStatus, HotkeyStatus } from "../types/app";

export type SetupState = "setup_api_key" | "setup_hotkey" | "ready";

export function computeSetupState(
  apiKeyStatus: ApiKeyStatus | null,
  hotkeyStatus: HotkeyStatus | null,
): SetupState {
  if (apiKeyStatus === null || !apiKeyStatus.configured) {
    return "setup_api_key";
  }

  if (hotkeyStatus === null || !hotkeyStatus.isRegistered) {
    return "setup_hotkey";
  }

  return "ready";
}

export function computeVisibleSetupState(
  apiKeyStatus: ApiKeyStatus | null,
  hotkeyStatus: HotkeyStatus | null,
  showHotkeyStepAfterApiKeySave: boolean,
): SetupState {
  const base = computeSetupState(apiKeyStatus, hotkeyStatus);

  if (base === "ready" && showHotkeyStepAfterApiKeySave) {
    return "setup_hotkey";
  }

  return base;
}
