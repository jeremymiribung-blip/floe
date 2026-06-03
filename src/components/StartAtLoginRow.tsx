import { useState } from "react";
import type { StartAtLoginError, StartAtLoginStatus } from "../types/app";

interface StartAtLoginRowProps {
  status: StartAtLoginStatus | null;
  onChange: (enabled: boolean) => Promise<void> | void;
  disabled?: boolean;
}

export function StartAtLoginRow({
  status,
  onChange,
  disabled = false,
}: StartAtLoginRowProps) {
  const [message, setMessage] = useState<string | null>(null);
  const enabled = status?.enabled ?? false;
  const unavailable = status?.available === false;
  const isDisabled = disabled || status === null || unavailable;
  const value = status === null ? "Loading" : enabled ? "On" : "Off";

  async function toggleStartAtLogin() {
    const nextEnabled = !enabled;

    try {
      await onChange(nextEnabled);
      setMessage(null);
    } catch (caught) {
      setMessage(startAtLoginErrorMessage(caught, nextEnabled));
    }
  }

  return (
    <div className="start-at-login-row">
      <div className="start-at-login-row__field">
        <span className="start-at-login-row__value">{value}</span>
        <button
          type="button"
          role="switch"
          aria-checked={enabled}
          className="start-at-login-row__toggle"
          onClick={() => void toggleStartAtLogin()}
          disabled={isDisabled}
        >
          {enabled ? "On" : "Off"}
        </button>
      </div>
      {unavailable ? (
        <p className="start-at-login-row__message">
          Start at login unavailable
        </p>
      ) : null}
      {message ? (
        <p className="start-at-login-row__message">{message}</p>
      ) : null}
    </div>
  );
}

function startAtLoginErrorMessage(caught: unknown, enabling: boolean): string {
  const error = caught as Partial<StartAtLoginError>;

  if (
    error.message === "Could not enable start at login" ||
    error.message === "Could not disable start at login" ||
    error.message === "Start at login unavailable"
  ) {
    return error.message;
  }

  if (error.code === "unavailable") {
    return "Start at login unavailable";
  }

  return enabling
    ? "Could not enable start at login"
    : "Could not disable start at login";
}
