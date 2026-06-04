import { GroqSetupStep } from "./GroqSetupStep";
import { HotkeySetupStep } from "./HotkeySetupStep";
import type { SetupState } from "../lib/setupState";
import type { HotkeyStatus } from "../types/app";

interface OnboardingViewProps {
  step: Exclude<SetupState, "ready">;
  hotkeyStatus: HotkeyStatus | null;
  onSaveGroq: (value: string) => Promise<void> | void;
  onChangeHotkey: (accelerator: string) => Promise<void> | void;
  onComplete: () => void;
  busy?: boolean;
}

export function OnboardingView({
  step,
  hotkeyStatus,
  onSaveGroq,
  onChangeHotkey,
  onComplete,
  busy = false,
}: OnboardingViewProps) {
  return (
    <section className="onboarding-view">
      <h1 className="wordmark">Floe</h1>
      {step === "setup_groq" ? (
        <GroqSetupStep busy={busy} onContinue={onSaveGroq} />
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
