interface StatusViewProps {
  status: string;
  hotkeyLabel: string;
  error: string | null;
  onOpenSettings: () => void;
}

export function StatusView({
  status,
  hotkeyLabel,
  error,
  onOpenSettings,
}: StatusViewProps) {
  return (
    <section className="status-view" aria-live="polite">
      <h1 className="wordmark">Floe</h1>
      <p className="status-view__status">{status}</p>
      <p className="status-view__hotkey">{hotkeyLabel}</p>
      {error ? <p className="status-view__error">{error}</p> : null}
      <button
        type="button"
        className="status-view__settings"
        onClick={onOpenSettings}
      >
        Settings
      </button>
    </section>
  );
}
