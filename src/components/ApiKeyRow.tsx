import { useState, type FormEvent } from "react";
import type { GroqApiKeyStatus, CerebrasApiKeyStatus } from "../types/app";

type Provider = "groq" | "cerebras";

interface ApiKeyRowProps {
  provider: Provider;
  label: string;
  status: GroqApiKeyStatus | CerebrasApiKeyStatus | null;
  onSave: (value: string) => Promise<void> | void;
  onClear: () => Promise<void> | void;
  disabled?: boolean;
}

export function ApiKeyRow({
  label,
  status,
  onSave,
  onClear,
  disabled = false,
}: ApiKeyRowProps) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const configured = status?.configured ?? false;
  const preview = status?.maskedPreview;

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    try {
      await onSave(value);
      setValue("");
      setEditing(false);
    } catch (caught) {
      const message =
        caught instanceof Error ? caught.message : "Could not save key.";
      setError(message);
    }
  }

  async function handleClear() {
    setError(null);
    try {
      await onClear();
    } catch (caught) {
      const message =
        caught instanceof Error ? caught.message : "Could not clear key.";
      setError(message);
    }
  }

  if (editing) {
    return (
      <div className="key-row key-row--editing">
        <form className="key-row__form" onSubmit={handleSubmit}>
          <label className="key-row__label" htmlFor={`${label}-key-input`}>
            {label}
          </label>
          <div className="key-row__field">
            <input
              id={`${label}-key-input`}
              className="key-row__input"
              type="password"
              autoComplete="off"
              value={value}
              onChange={(event) => setValue(event.target.value)}
              autoFocus
            />
            <button
              type="submit"
              className="key-row__button key-row__button--primary"
              disabled={value.trim().length === 0 || disabled}
            >
              Save
            </button>
            <button
              type="button"
              className="key-row__button"
              onClick={() => {
                setValue("");
                setError(null);
                setEditing(false);
              }}
              disabled={disabled}
            >
              Cancel
            </button>
          </div>
          {error ? <p className="key-row__error">{error}</p> : null}
        </form>
      </div>
    );
  }

  return (
    <div className="key-row">
      <div className="key-row__label">{label}</div>
      <div className="key-row__field">
        <span className="key-row__status">
          {configured
            ? preview
              ? `Configured (${preview})`
              : "Configured"
            : "Missing"}
        </span>
        <button
          type="button"
          className="key-row__button key-row__button--primary"
          onClick={() => {
            setError(null);
            setEditing(true);
          }}
          disabled={disabled}
        >
          {configured ? "Change" : "Add"}
        </button>
        {configured ? (
          <button
            type="button"
            className="key-row__button"
            onClick={handleClear}
            disabled={disabled}
          >
            Clear
          </button>
        ) : null}
      </div>
      {error ? <p className="key-row__error">{error}</p> : null}
    </div>
  );
}
