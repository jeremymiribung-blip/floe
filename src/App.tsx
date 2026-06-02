import { Clipboard, Mic, Settings, WandSparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { getAppStatus, getSettingsStub, runManualTestStub } from "./lib/tauri";
import { statusLabel } from "./lib/status";
import type {
  AppState,
  AppStatus,
  ManualTestResult,
  SettingsStub,
} from "./types/app";

export default function App() {
  const [appState, setAppState] = useState<AppState>("loading");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settings, setSettings] = useState<SettingsStub | null>(null);
  const [manualResult, setManualResult] = useState<ManualTestResult | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([getAppStatus(), getSettingsStub()])
      .then(([appStatus, settingsStub]) => {
        setStatus(appStatus);
        setSettings(settingsStub);
        setAppState("ready");
      })
      .catch(() => {
        setError("Floe could not load setup stubs.");
        setAppState("error");
      });
  }, []);

  async function handleManualTest(action: string) {
    try {
      setAppState("checking");
      setError(null);
      setManualResult(await runManualTestStub(action));
      setAppState("ready");
    } catch {
      setError(`The ${action} placeholder check failed.`);
      setAppState("error");
    }
  }

  return (
    <main>
      <div className="app-shell">
        <header className="app-header">
          <div>
            <p className="eyebrow">Desktop transcription</p>
            <h1>Floe</h1>
          </div>
          <div className={`status-pill status-pill-${appState}`}>
            <span aria-hidden="true" />
            {statusLabel(appState)}
          </div>
        </header>

        <section className="status-panel" aria-live="polite">
          <div>
            <p className="section-label">Status</p>
            <h2>{status?.appName ?? "Floe"} setup scaffold</h2>
          </div>
          <p>{error ?? status?.message ?? "Loading setup stubs..."}</p>
          {manualResult ? (
            <p className="manual-result">{manualResult.message}</p>
          ) : null}
        </section>

        <section className="settings-panel">
          <div>
            <p className="section-label">Settings</p>
            <h2>
              <Settings aria-hidden="true" />
              Placeholder
            </h2>
          </div>
          <dl>
            <div>
              <dt>Groq API key</dt>
              <dd>
                {settings?.hasGroqApiKey ? "Configured" : "Not configured"}
              </dd>
            </div>
            <div>
              <dt>Global hotkey</dt>
              <dd>{settings?.hotkeyLabel ?? "Loading"}</dd>
            </div>
            <div>
              <dt>Secret storage</dt>
              <dd>{settings?.storageLabel ?? "Loading"}</dd>
            </div>
          </dl>
        </section>

        <section className="manual-panel">
          <div>
            <p className="section-label">Future manual testing</p>
            <h2>Stub buttons</h2>
          </div>
          <div className="actions">
            <button type="button" onClick={() => handleManualTest("recording")}>
              <Mic aria-hidden="true" />
              Recording stub
            </button>
            <button
              type="button"
              onClick={() => handleManualTest("transcription")}
            >
              <WandSparkles aria-hidden="true" />
              Transcription stub
            </button>
            <button type="button" onClick={() => handleManualTest("paste")}>
              <Clipboard aria-hidden="true" />
              Paste stub
            </button>
          </div>
          <button
            className="secondary-action"
            type="button"
            disabled
            aria-disabled="true"
          >
            Push-to-talk coming later
          </button>
        </section>
      </div>
    </main>
  );
}
