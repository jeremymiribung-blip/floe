import { ApiKeyRow } from "./ApiKeyRow";
import { HotkeyRow } from "./HotkeyRow";
import { PrivacyNote } from "./PrivacyNote";
import type {
  CerebrasApiKeyStatus,
  GroqApiKeyStatus,
  HotkeyStatus,
} from "../types/app";

interface SettingsViewProps {
  groqStatus: GroqApiKeyStatus | null;
  cerebrasStatus: CerebrasApiKeyStatus | null;
  hotkeyStatus: HotkeyStatus | null;
  onClose: () => void;
  onSaveGroq: (value: string) => Promise<void> | void;
  onClearGroq: () => Promise<void> | void;
  onSaveCerebras: (value: string) => Promise<void> | void;
  onClearCerebras: () => Promise<void> | void;
  onChangeHotkey: (accelerator: string) => Promise<void> | void;
  onResetHotkey: () => Promise<void> | void;
  busy?: boolean;
}

export function SettingsView({
  groqStatus,
  cerebrasStatus,
  hotkeyStatus,
  onClose,
  onSaveGroq,
  onClearGroq,
  onSaveCerebras,
  onClearCerebras,
  onChangeHotkey,
  onResetHotkey,
  busy = false,
}: SettingsViewProps) {
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
        <h2 className="settings-view__heading">API Keys</h2>
        <ApiKeyRow
          provider="groq"
          label="Groq"
          status={groqStatus}
          onSave={onSaveGroq}
          onClear={onClearGroq}
          disabled={busy}
        />
        <ApiKeyRow
          provider="cerebras"
          label="Cerebras"
          status={cerebrasStatus}
          onSave={onSaveCerebras}
          onClear={onClearCerebras}
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
        <h2 className="settings-view__heading">Privacy</h2>
        <PrivacyNote
          items={[
            "Audio → Groq",
            "Text → Cerebras",
            "Keys stored locally",
            "No audio saved",
          ]}
        />
      </div>
    </section>
  );
}
