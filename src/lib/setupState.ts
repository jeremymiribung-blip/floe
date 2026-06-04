import type { GroqApiKeyStatus, HotkeyStatus } from "../types/app";

export type SetupState = "setup_groq" | "setup_hotkey" | "ready";

export function computeSetupState(
  groqStatus: GroqApiKeyStatus | null,
  hotkeyStatus: HotkeyStatus | null,
): SetupState {
  if (groqStatus === null || !groqStatus.configured) {
    return "setup_groq";
  }

  if (hotkeyStatus === null || !hotkeyStatus.isRegistered) {
    return "setup_hotkey";
  }

  return "ready";
}

export function isReady(state: SetupState): boolean {
  return state === "ready";
}
