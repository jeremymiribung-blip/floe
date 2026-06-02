import { invoke } from "@tauri-apps/api/core";
import type { AppStatus, ManualTestResult, SettingsStub } from "../types/app";

const browserStatus: AppStatus = {
  appName: "Floe",
  status: "setup_only",
  message:
    "Initial scaffold is ready. Runtime transcription features are not implemented yet.",
};

const browserSettings: SettingsStub = {
  hasGroqApiKey: false,
  hotkeyLabel: "Not configured",
  storageLabel: "Keychain storage not wired yet",
};

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export function getAppStatus(): Promise<AppStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserStatus);
  }

  return invoke("get_app_status");
}

export function getSettingsStub(): Promise<SettingsStub> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserSettings);
  }

  return invoke("get_settings_stub");
}

export function runManualTestStub(action: string): Promise<ManualTestResult> {
  if (!isTauriRuntime()) {
    return Promise.resolve({
      action,
      message: `${action} is a placeholder for a future implementation task.`,
    });
  }

  return invoke("run_manual_test_stub", { action });
}
