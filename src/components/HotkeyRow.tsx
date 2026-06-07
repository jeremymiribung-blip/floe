import { useCallback, useEffect, useState } from "react";
import { captureHotkey } from "../lib/hotkeyCapture";
import type { HotkeyStatus } from "../types/app";

interface HotkeyRowProps {
  hotkeyStatus: HotkeyStatus | null;
  onChange: (accelerator: string) => Promise<void> | void;
  onReset: () => Promise<void> | void;
  disabled?: boolean;
}

export function HotkeyRow({
  hotkeyStatus,
  onChange,
  onReset,
  disabled = false,
}: HotkeyRowProps) {
  const [capturing, setCapturing] = useState(false);
  const [captureMessage, setCaptureMessage] = useState<string | null>(null);

  const label = hotkeyStatus === null ? "Loading" : hotkeyStatus.label;
  const showUnavailable =
    hotkeyStatus !== null &&
    !hotkeyStatus.isRegistered &&
    hotkeyStatus.error !== null;

  const cancelCapture = useCallback(() => {
    setCapturing(false);
    setCaptureMessage(null);
  }, []);

  const saveCapture = useCallback(
    async (accelerator: string) => {
      try {
        await onChange(accelerator);
        setCapturing(false);
        setCaptureMessage(null);
      } catch (caught) {
        const message =
          caught instanceof Error
            ? caught.message
            : "This shortcut is not supported.";
        setCaptureMessage(message);
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
        setCaptureMessage(
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
      <div className="hotkey-row hotkey-row--capturing">
        <div className="hotkey-row__label">Hotkey</div>
        <div className="hotkey-row__field">
          <span className="hotkey-row__value">Press shortcut</span>
          <button
            type="button"
            className="hotkey-row__button"
            onClick={cancelCapture}
            disabled={disabled}
          >
            Cancel
          </button>
        </div>
        {captureMessage ? (
          <p className="hotkey-row__message">{captureMessage}</p>
        ) : null}
      </div>
    );
  }

  return (
    <div className="hotkey-row">
      <div className="hotkey-row__label">Hotkey</div>
      <div className="hotkey-row__field">
        <span className="hotkey-row__value">{label}</span>
        <button
          type="button"
          className="hotkey-row__button hotkey-row__button--primary"
          onClick={() => {
            setCaptureMessage(null);
            setCapturing(true);
          }}
          disabled={disabled}
        >
          Change
        </button>
        <button
          type="button"
          className="hotkey-row__button"
          onClick={onReset}
          disabled={disabled}
        >
          Reset
        </button>
      </div>
      {captureMessage ? (
        <p className="hotkey-row__message">{captureMessage}</p>
      ) : null}
      {showUnavailable ? (
        <p className="hotkey-row__message">{hotkeyStatus.error}</p>
      ) : null}
    </div>
  );
}
