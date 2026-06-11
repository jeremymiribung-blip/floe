import { ApiKeyRow } from "./ApiKeyRow";
import { HotkeyRow } from "./HotkeyRow";
import { PrivacyNote } from "./PrivacyNote";
import { StartAtLoginRow } from "./StartAtLoginRow";
import type {
  ApiKeyStatus,
  HotkeyStatus,
  StartAtLoginStatus,
} from "../types/app";

interface SettingsViewProps {
  apiKeyStatus: ApiKeyStatus | null;
  hotkeyStatus: HotkeyStatus | null;
  startAtLoginStatus: StartAtLoginStatus | null;
  onClose: () => void;
  onSaveApiKey: (value: string) => Promise<void> | void;
  onClearApiKey: () => Promise<void> | void;
  onChangeHotkey: (accelerator: string) => Promise<void> | void;
  onResetHotkey: () => Promise<void> | void;
  onSetStartAtLogin: (enabled: boolean) => Promise<void> | void;
  busy?: boolean;
}

export function SettingsView({
  apiKeyStatus,
  hotkeyStatus,
  startAtLoginStatus,
  onClose,
  onSaveApiKey,
  onClearApiKey,
  onChangeHotkey,
  onResetHotkey,
  onSetStartAtLogin,
  busy = false,
}: SettingsViewProps) {
  const privacyItems = [
    "Audio → API",
    "Text → API",
    "Keys stored locally",
    "No audio saved",
  ];

  return (
    <section className="settings-view">
      <header className="settings-view__header">
        <h1 className="wordmark">Floe</h1>
        <button
          type="button"
          className="settings-view__close"
          onClick={onClose}
        >
          Close
        </button>
      </header>

      <div className="settings-view__group">
        <h2 className="settings-view__heading">API Key</h2>
        <ApiKeyRow
          label="API"
          status={apiKeyStatus}
          onSave={onSaveApiKey}
          onClear={onClearApiKey}
          disabled={busy}
        />
      </div>

      <div className="settings-view__group">
        <h2 className="settings-view__heading">Hotkey</h2>
        <HotkeyRow
          hotkeyStatus={hotkeyStatus}
          onChange={onChangeHotkey}
          onReset={onResetHotkey}
          disabled={busy}
        />
      </div>

      <div className="settings-view__group">
        <h2 className="settings-view__heading">Start at login</h2>
        <StartAtLoginRow
          status={startAtLoginStatus}
          onChange={onSetStartAtLogin}
          disabled={busy}
        />
      </div>

      <div className="settings-view__group">
        <h2 className="settings-view__heading">Privacy</h2>
        <PrivacyNote items={privacyItems} />
      </div>
    </section>
  );
}
