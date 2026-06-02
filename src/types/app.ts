export type AppState = "loading" | "ready" | "checking" | "error";

export interface AppStatus {
  appName: "Floe";
  status: "setup_only";
  message: string;
}

export interface SettingsStub {
  hasGroqApiKey: boolean;
  hotkeyLabel: string;
  storageLabel: string;
}

export interface ManualTestResult {
  action: string;
  message: string;
}
