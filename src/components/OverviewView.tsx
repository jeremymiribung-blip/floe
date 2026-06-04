interface OverviewViewProps {
  status: string;
  hotkeyLabel: string;
  onOpenSettings: () => void;
}

export function OverviewView({
  status,
  hotkeyLabel,
  onOpenSettings,
}: OverviewViewProps) {
  return (
    <section className="overview-view" aria-live="polite">
      <h1 className="wordmark">Floe</h1>
      <p className="overview-view__status">{status}</p>
      <p className="overview-view__hotkey">{hotkeyLabel}</p>
      <button
        type="button"
        className="overview-view__settings"
        onClick={onOpenSettings}
      >
        Settings
      </button>
    </section>
  );
}
