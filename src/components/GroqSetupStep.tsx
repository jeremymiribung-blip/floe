import { useState, type FormEvent } from "react";

interface GroqSetupStepProps {
  onContinue: (value: string) => Promise<void> | void;
  busy?: boolean;
}

export function GroqSetupStep({
  onContinue,
  busy = false,
}: GroqSetupStepProps) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (value.trim().length === 0) {
      return;
    }
    setError(null);
    setSubmitting(true);
    try {
      await onContinue(value);
    } catch {
      setError("Could not save key");
    } finally {
      setSubmitting(false);
    }
  }

  const disabled = busy || submitting || value.trim().length === 0;

  return (
    <form className="setup-step" onSubmit={handleSubmit}>
      <h2 className="setup-step__label">Groq API key</h2>
      <div className="setup-step__field">
        <input
          className="setup-step__input"
          type="password"
          autoComplete="off"
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            if (error !== null) {
              setError(null);
            }
          }}
          autoFocus
          disabled={busy || submitting}
        />
        <button
          type="submit"
          className="setup-step__button setup-step__button--primary"
          disabled={disabled}
        >
          Continue
        </button>
      </div>
      {error ? <p className="setup-step__error">{error}</p> : null}
    </form>
  );
}
