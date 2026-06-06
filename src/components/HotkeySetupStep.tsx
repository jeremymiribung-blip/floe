import { useCallback, useEffect, useState } from "react";
import { captureHotkey } from "../lib/hotkeyCapture";
import type { HotkeyStatus } from "../types/app";

interface HotkeySetupStepProps {
  hotkeyStatus: HotkeyStatus | null;
  onChange: (accelerator: string) => Promise<void> | void;
  onContinue: () => void;
  busy?: boolean;
}

export function HotkeySetupStep({
  hotkeyStatus,
  onChange,
  onContinue,
  busy = false,
}: HotkeySetupStepProps) {
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const label =
    hotkeyStatus === null
      ? "Loading"
      : hotkeyStatus.isRegistered
        ? hotkeyStatus.label
        : "Hotkey unavailable";
  const canContinue =
    !busy && !capturing && hotkeyStatus?.isRegistered === true;

  const cancelCapture = useCallback(() => {
    setCapturing(false);
    setError(null);
  }, []);

  const saveCapture = useCallback(
    async (accelerator: string) => {
      try {
        await onChange(accelerator);
        setError(null);
        setCapturing(false);
      } catch {
        setError("Hotkey unavailable");
        setCapturing(false);
      }
    },
    [onChange],
  );

  useEffect(() => {
    if (!capturing) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        cancelCapture();
        return;
      }

      try {
        const captured = captureHotkey(event);
        void saveCapture(captured.accelerator);
      } catch (caught) {
        setError(
          caught instanceof Error
            ? caught.message
            : "This shortcut is not supported.",
        );
        setCapturing(false);
      }
    }

    window.addEventListener("keydown", handleKeydown, true);

    return () => {
      window.removeEventListener("keydown", handleKeydown, true);
    };
  }, [cancelCapture, capturing, saveCapture]);

  if (capturing) {
    return (
      <div className="setup-step">
        <h2 className="setup-step__label">Hotkey</h2>
        <div className="setup-step__field">
          <span className="setup-step__value">Press shortcut</span>
          <button
            type="button"
            className="setup-step__button"
            onClick={cancelCapture}
            disabled={busy}
          >
            Cancel
          </button>
        </div>
        {error ? <p className="setup-step__error">{error}</p> : null}
      </div>
    );
  }

  return (
    <div className="setup-step">
      <h2 className="setup-step__label">Hotkey</h2>
      <div className="setup-step__field">
        <span className="setup-step__value">{label}</span>
        <button
          type="button"
          className="setup-step__button setup-step__button--primary"
          onClick={() => {
            setError(null);
            setCapturing(true);
          }}
          disabled={busy}
        >
          Change
        </button>
        <button
          type="button"
          className="setup-step__button"
          onClick={onContinue}
          disabled={!canContinue}
        >
          Continue
        </button>
      </div>
      {error ? <p className="setup-step__error">{error}</p> : null}
    </div>
  );
}
