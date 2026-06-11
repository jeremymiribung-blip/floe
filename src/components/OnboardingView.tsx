import { ApiKeySetupStep } from "./ApiKeySetupStep";
import { HotkeySetupStep } from "./HotkeySetupStep";
import type { SetupState } from "../lib/setupState";
import type { HotkeyStatus } from "../types/app";

interface OnboardingViewProps {
  step: Exclude<SetupState, "ready">;
  hotkeyStatus: HotkeyStatus | null;
  onSaveApiKey: (value: string) => Promise<void> | void;
  onChangeHotkey: (accelerator: string) => Promise<void> | void;
  onComplete: () => void;
  busy?: boolean;
}

export function OnboardingView({
  step,
  hotkeyStatus,
  onSaveApiKey,
  onChangeHotkey,
  onComplete,
  busy = false,
}: OnboardingViewProps) {
  return (
    <section className="onboarding-view">
      <h1 className="wordmark">Floe</h1>
      {step === "setup_api_key" ? (
        <ApiKeySetupStep busy={busy} onContinue={onSaveApiKey} />
      ) : (
        <HotkeySetupStep
          hotkeyStatus={hotkeyStatus}
          busy={busy}
          onChange={onChangeHotkey}
          onContinue={onComplete}
        />
      )}
    </section>
  );
}
