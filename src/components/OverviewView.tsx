import { useState } from "react";

interface OverviewViewProps {
  status: string;
  hotkeyLabel: string;
  onOpenSettings: () => void;
  diagnosticsJson: string | null;
  onCopyDiagnostics: (json: string) => Promise<void> | void;
}

export function OverviewView({
  status,
  hotkeyLabel,
  onOpenSettings,
  diagnosticsJson,
  onCopyDiagnostics,
}: OverviewViewProps) {
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [copyStatus, setCopyStatus] = useState<string | null>(null);

  async function handleCopyDiagnostics() {
    if (!diagnosticsJson) {
      return;
    }

    await onCopyDiagnostics(diagnosticsJson);
    setCopyStatus("Copied");
  }

  return (
    <section className="overview-view" aria-live="polite">
      <h1 className="wordmark">Floe</h1>
      <p className="overview-view__status">{status}</p>
      <p className="overview-view__hotkey">{hotkeyLabel}</p>
      <div className="overview-view__actions">
        <button
          type="button"
          className="overview-view__settings"
          onClick={onOpenSettings}
        >
          Settings
        </button>
        <button
          type="button"
          className="overview-view__diagnostics"
          onClick={() => {
            setCopyStatus(null);
            setDiagnosticsOpen(true);
          }}
        >
          Diagnostics
        </button>
      </div>

      {diagnosticsOpen ? (
        <div
          className="diagnostics-popover"
          role="dialog"
          aria-label="Diagnostics"
        >
          <pre className="diagnostics-popover__json">
            {diagnosticsJson ?? "No diagnostics yet"}
          </pre>
          <div className="diagnostics-popover__actions">
            <button
              type="button"
              className="diagnostics-popover__button"
              onClick={() => void handleCopyDiagnostics()}
              disabled={!diagnosticsJson}
            >
              Copy
            </button>
            <button
              type="button"
              className="diagnostics-popover__button"
              onClick={() => setDiagnosticsOpen(false)}
            >
              Close
            </button>
            {copyStatus ? (
              <span className="diagnostics-popover__status">{copyStatus}</span>
            ) : null}
          </div>
        </div>
      ) : null}
    </section>
  );
}
