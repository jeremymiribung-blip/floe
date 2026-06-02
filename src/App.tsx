import {
  Activity,
  Clipboard,
  Info,
  Mic,
  Settings,
  Square,
  WandSparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import { formatRecordingInfo } from "./lib/recording";
import {
  getAppStatus,
  getLatestRecordingInfo,
  getRecordingStatus,
  getSettingsStub,
  runManualTestStub,
  startRecording,
  stopRecording,
} from "./lib/tauri";
import { statusLabel } from "./lib/status";
import type {
  AppState,
  AppStatus,
  ManualTestResult,
  RecordingError,
  RecordingInfo,
  RecordingStatus,
  SettingsStub,
} from "./types/app";

export default function App() {
  const [appState, setAppState] = useState<AppState>("loading");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settings, setSettings] = useState<SettingsStub | null>(null);
  const [manualResult, setManualResult] = useState<ManualTestResult | null>(
    null,
  );
  const [recordingStatus, setRecordingStatus] =
    useState<RecordingStatus | null>(null);
  const [latestRecording, setLatestRecording] = useState<RecordingInfo | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([getAppStatus(), getSettingsStub(), getRecordingStatus()])
      .then(([appStatus, settingsStub, currentRecordingStatus]) => {
        setStatus(appStatus);
        setSettings(settingsStub);
        setRecordingStatus(currentRecordingStatus);
        setLatestRecording(currentRecordingStatus.latestRecording);
        setAppState(currentRecordingStatus.isRecording ? "recording" : "ready");
      })
      .catch(() => {
        setError("Floe could not load setup state.");
        setAppState("error");
      });
  }, []);

  function applyRecordingStatus(nextStatus: RecordingStatus) {
    setRecordingStatus(nextStatus);
    setLatestRecording(nextStatus.latestRecording);
    setAppState(nextStatus.isRecording ? "recording" : "ready");
  }

  function recordingErrorMessage(action: string, caught: unknown): string {
    const recordingError = caught as Partial<RecordingError>;

    if (typeof recordingError.message === "string") {
      return recordingError.message;
    }

    return `The ${action} recording check failed.`;
  }

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

  async function handleStartRecording() {
    try {
      setAppState("checking");
      setError(null);
      setManualResult(null);
      applyRecordingStatus(await startRecording());
    } catch (caught) {
      setError(recordingErrorMessage("start", caught));
      setAppState("error");
    }
  }

  async function handleStopRecording() {
    try {
      setAppState("checking");
      setError(null);
      const info = await stopRecording();
      setLatestRecording(info);
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("stop", caught));
      setAppState("error");
    }
  }

  async function handleRefreshRecordingStatus() {
    try {
      setAppState("checking");
      setError(null);
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("status", caught));
      setAppState("error");
    }
  }

  async function handleLatestRecordingInfo() {
    try {
      setAppState("checking");
      setError(null);
      setLatestRecording(await getLatestRecordingInfo());
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("latest info", caught));
      setAppState("error");
    }
  }

  const isRecording = recordingStatus?.isRecording ?? false;
  const safeLatestRecording =
    latestRecording ?? recordingStatus?.latestRecording ?? null;

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
            <p className="section-label">Manual testing</p>
            <h2>
              <Mic aria-hidden="true" />
              Recording controls
            </h2>
          </div>
          <div className="actions">
            <button
              type="button"
              disabled={isRecording}
              onClick={handleStartRecording}
            >
              <Mic aria-hidden="true" />
              Start
            </button>
            <button
              type="button"
              disabled={!isRecording}
              onClick={handleStopRecording}
            >
              <Square aria-hidden="true" />
              Stop
            </button>
            <button type="button" onClick={handleRefreshRecordingStatus}>
              <Activity aria-hidden="true" />
              Status
            </button>
            <button type="button" onClick={handleLatestRecordingInfo}>
              <Info aria-hidden="true" />
              Latest info
            </button>
          </div>

          <dl className="recording-metadata">
            <div>
              <dt>Recording</dt>
              <dd>{isRecording ? "Active" : "Idle"}</dd>
            </div>
            <div>
              <dt>Sample rate</dt>
              <dd>{recordingStatus?.sampleRate ?? "None"}</dd>
            </div>
            <div>
              <dt>Input channels</dt>
              <dd>{recordingStatus?.inputChannels ?? "None"}</dd>
            </div>
            <div>
              <dt>Duration cap</dt>
              <dd>{recordingStatus?.maxDurationSeconds ?? 120}s</dd>
            </div>
          </dl>

          {safeLatestRecording ? (
            <p className="manual-result">
              {formatRecordingInfo(safeLatestRecording)}
            </p>
          ) : null}

          <div className="actions">
            <button
              className="secondary-button"
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
