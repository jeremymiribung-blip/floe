import { useEffect, useCallback, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Activity, X } from "lucide-react";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Switch } from "../components/ui/switch";
import { DiagnosticsSection } from "../components/DiagnosticsSection";
import UpdateSection from "../components/UpdateSection";
import {
  isTauriRuntime,
  saveApiKey,
  validateApiKey,
  setHotkey as setHotkeyBackend,
  setStartAtLoginEnabled,
  getAudioDevices,
  getAppSettings,
  saveAppSettings,
} from "../lib/tauri";
import {
  errorMessage,
  isKeychainError,
  KEYCHAIN_UNAVAILABLE_MESSAGE,
  logCritical,
} from "../lib/errorLog";
import useFloeStore from "../stores/useFloeStore";
import type { AudioDevice } from "../types/app";
import { useReducedMotionPreference } from "../hooks/useReducedMotionPreference";
import { cn } from "../lib/utils";

function buildHotkeyString(e: KeyboardEvent): string | null {
  const { ctrlKey, altKey, shiftKey, metaKey, key, code } = e;

  if (!ctrlKey && !altKey && !shiftKey && !metaKey) return null;

  let keyName: string | undefined;
  if (code.startsWith("Key")) {
    keyName = code.slice(3);
  } else if (code.startsWith("Digit")) {
    keyName = code.slice(5);
  } else if (key === " ") {
    keyName = "Space";
  } else if (key === "Escape") {
    keyName = "Esc";
  } else if (key === "Tab") {
    keyName = "Tab";
  } else if (key.startsWith("Arrow")) {
    keyName = key;
  } else if (
    code.startsWith("F") &&
    code.length >= 2 &&
    !isNaN(Number(code[1]))
  ) {
    keyName = code;
  } else {
    return null;
  }

  if (!keyName) return null;

  const parts: string[] = [];
  if (ctrlKey) parts.push("Ctrl");
  if (altKey) parts.push("Alt");
  if (shiftKey) parts.push("Shift");
  if (metaKey) parts.push("Meta");
  parts.push(keyName);

  return parts.join("+");
}

interface SettingsWindowProps {
  onClose?: () => void;
}

export default function SettingsWindow({ onClose }: SettingsWindowProps) {
  const apiKey = useFloeStore((s) => s.apiKey);
  const hotkey = useFloeStore((s) => s.hotkey);
  const isHotkeyCaptureActive = useFloeStore((s) => s.isHotkeyCaptureActive);
  const launchOnStartup = useFloeStore((s) => s.launchOnStartup);
  const apiKeyConfigured = useFloeStore((s) => s.apiKeyConfigured);
  const audioDevices = useFloeStore((s) => s.audioDevices);
  const selectedAudioDeviceId = useFloeStore((s) => s.selectedAudioDeviceId);
  const setApiKey = useFloeStore((s) => s.setApiKey);
  const setApiKeyStatus = useFloeStore((s) => s.setApiKeyStatus);
  const setHotkey = useFloeStore((s) => s.setHotkey);
  const setHotkeyStatus = useFloeStore((s) => s.setHotkeyStatus);
  const setLaunchOnStartup = useFloeStore((s) => s.setLaunchOnStartup);
  const setAudioDevices = useFloeStore((s) => s.setAudioDevices);
  const setSelectedAudioDeviceId = useFloeStore(
    (s) => s.setSelectedAudioDeviceId,
  );
  const startHotkeyCapture = useFloeStore((s) => s.startHotkeyCapture);
  const stopHotkeyCapture = useFloeStore((s) => s.stopHotkeyCapture);
  const closeSettings = useFloeStore((s) => s.closeSettings);
  const updateInfo = useFloeStore((s) => s.updateInfo);

  const reducedMotion = useReducedMotionPreference();
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [isLoadingDevices, setIsLoadingDevices] = useState(false);
  const [deviceLoadError, setDeviceLoadError] = useState<string | null>(null);
  const [isValidating, setIsValidating] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const saveErrorTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showSaveError = useCallback((message: string) => {
    setSaveError(message);
    if (saveErrorTimerRef.current) clearTimeout(saveErrorTimerRef.current);
    saveErrorTimerRef.current = setTimeout(() => setSaveError(null), 6000);
  }, []);

  const apiKeyRef = useRef(apiKey);
  apiKeyRef.current = apiKey;

  const validateAndSaveApiKey = useCallback(
    async (key: string) => {
      if (!isTauriRuntime() || !key) return;

      setIsValidating(true);
      setValidationError(null);

      try {
        const isValid = await validateApiKey(key);
        if (isValid) {
          await saveApiKey(key);
          setApiKeyStatus(true, "Configured");
        } else {
          setValidationError(
            "Invalid API key. Please check your Groq console.",
          );
        }
      } catch (err) {
        logCritical("settings validateAndSaveApiKey", err);
        // Differentiate a keyring storage failure from a generic network
        // error so users with a locked/unavailable keychain don't go chasing
        // a network problem that doesn't exist.
        if (isKeychainError(err)) {
          setValidationError(KEYCHAIN_UNAVAILABLE_MESSAGE);
        } else {
          setValidationError(
            `Could not validate or save your API key: ${errorMessage(err)}. Check your network connection and try again.`,
          );
        }
      } finally {
        setIsValidating(false);
      }
    },
    [setApiKeyStatus],
  );

  const handleClose = async () => {
    const key = apiKeyRef.current;
    // Drain any pending unsaved draft before hiding the window so a save
    // failure isn't silently dropped. The await is bounded by the
    // validate_api_key HTTP timeout (currently 10s on the backend).
    if (key) {
      await validateAndSaveApiKey(key);
    }
    closeSettings();
    onClose?.();
  };

  const handleApiKeyBlur = useCallback(() => {
    const key = apiKeyRef.current;
    if (key) {
      validateAndSaveApiKey(key);
    }
  }, [validateAndSaveApiKey]);

  // ── Keydown listener for hotkey capture ────────────────
  const handleKeyDown = useCallback(
    async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const combo = buildHotkeyString(e);
      if (!combo) return;

      // Capture the previous state so we can roll back on backend failure.
      const previousHotkey = useFloeStore.getState().hotkey;
      const previousRegistered = useFloeStore.getState().hotkeyRegistered;
      setHotkey(combo);
      stopHotkeyCapture();

      if (!isTauriRuntime()) {
        setHotkeyStatus(combo, true);
        return;
      }

      try {
        const status = await setHotkeyBackend(combo);
        if (!status || !status.isRegistered) {
          setHotkeyStatus(previousHotkey, previousRegistered);
          showSaveError(status?.error ?? `Could not register hotkey: ${combo}`);
          return;
        }
        setHotkeyStatus(status.label || combo, status.isRegistered);
      } catch (err) {
        logCritical("settings setHotkeyBackend", err);
        setHotkeyStatus(previousHotkey, previousRegistered);
        showSaveError(`Could not register hotkey: ${errorMessage(err)}`);
      }
    },
    [setHotkey, setHotkeyStatus, showSaveError, stopHotkeyCapture],
  );

  useEffect(() => {
    if (!isHotkeyCaptureActive) return;
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isHotkeyCaptureActive, handleKeyDown]);

  const handleHotkeyPillClick = () => {
    if (!isHotkeyCaptureActive) startHotkeyCapture();
  };

  const skipCleanup = useFloeStore((s) => s.skipCleanup);
  const setSkipCleanup = useFloeStore((s) => s.setSkipCleanup);

  const handleLaunchOnStartupChange = useCallback(
    async (checked: boolean) => {
      const previous = useFloeStore.getState().launchOnStartup;
      setLaunchOnStartup(checked);
      if (!isTauriRuntime()) return;

      try {
        await setStartAtLoginEnabled(checked);
      } catch (err) {
        logCritical("settings setStartAtLoginEnabled", err);
        setLaunchOnStartup(previous);
        showSaveError(
          `Could not ${checked ? "enable" : "disable"} start at login: ${errorMessage(err)}`,
        );
      }
    },
    [setLaunchOnStartup, showSaveError],
  );

  useEffect(() => {
    return () => {
      if (saveErrorTimerRef.current) clearTimeout(saveErrorTimerRef.current);
    };
  }, []);

  const handleSkipCleanupChange = useCallback(
    async (checked: boolean) => {
      const previous = useFloeStore.getState().skipCleanup;
      setSkipCleanup(checked);
      if (isTauriRuntime()) {
        try {
          const settings = await getAppSettings();
          await saveAppSettings({ ...settings, skipCleanup: checked });
        } catch (err) {
          setSkipCleanup(previous);
          const message = err instanceof Error ? err.message : String(err);
          showSaveError(`Failed to save settings: ${message}`);
        }
      }
    },
    [setSkipCleanup, showSaveError],
  );

  // Load audio devices on mount
  useEffect(() => {
    let mounted = true;
    const loadDevices = async () => {
      if (!isTauriRuntime()) return;
      setIsLoadingDevices(true);
      setDeviceLoadError(null);
      try {
        const devices = await getAudioDevices();
        if (mounted) {
          setAudioDevices(devices);
          if (!selectedAudioDeviceId && devices.length > 0) {
            setSelectedAudioDeviceId(devices[0].id);
          }
        }
      } catch (err) {
        logCritical("settings getAudioDevices", err);
        if (mounted) {
          setDeviceLoadError(
            `Could not load input devices: ${errorMessage(err)}`,
          );
        }
      } finally {
        if (mounted) setIsLoadingDevices(false);
      }
    };
    void loadDevices();
    return () => {
      mounted = false;
    };
  }, [
    isTauriRuntime,
    setAudioDevices,
    setSelectedAudioDeviceId,
    selectedAudioDeviceId,
  ]);

  const handleAudioDeviceChange = useCallback(
    async (deviceId: string) => {
      const previous = useFloeStore.getState().selectedAudioDeviceId;
      setSelectedAudioDeviceId(deviceId);
      if (isTauriRuntime()) {
        try {
          const settings = await getAppSettings();
          await saveAppSettings({ ...settings, deviceId });
        } catch (err) {
          setSelectedAudioDeviceId(previous);
          const message = err instanceof Error ? err.message : String(err);
          showSaveError(`Failed to save settings: ${message}`);
        }
      }
    },
    [setSelectedAudioDeviceId, showSaveError],
  );

  const cardEnterTransition = reducedMotion
    ? { duration: 0 }
    : { duration: 0.5, ease: [0.16, 1, 0.3, 1] as const };
  const diagRevealTransition = reducedMotion
    ? { duration: 0 }
    : { duration: 0.32, ease: [0.16, 1, 0.3, 1] as const };

  return (
    <div className="floe-window flex h-screen flex-col overflow-hidden">
      {/* ── Titlebar ────────────────────────────────────── */}
      <div
        className="floe-titlebar flex items-center justify-between"
        data-tauri-drag-region
      >
        <span className="floe-eyebrow">Floe Settings</span>
        <button
          type="button"
          className="flex size-7 items-center justify-center rounded-md border border-transparent text-white/50 transition-colors hover:border-white/10 hover:bg-white/5 hover:text-white/80"
          onClick={handleClose}
          aria-label="Close settings"
        >
          <X width={12} height={12} strokeWidth={1.5} strokeLinecap="round" />
        </button>
      </div>

      {/* ── Body ────────────────────────────────────────── */}
      <main className="flex flex-1 flex-col overflow-y-auto">
        <div className="mx-auto flex w-full max-w-[600px] flex-col gap-10 px-8 py-12">
          {/* ── General Card ──────────────────────────────── */}
          <motion.section
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.04, ...cardEnterTransition }}
            className="floe-card-elevated flex flex-col gap-7 p-8"
          >
            <div className="flex flex-col gap-2">
              <span className="floe-eyebrow">General</span>
              <h2 className="text-lg font-medium leading-snug text-white/90">
                Speech-to-text account
              </h2>
              <p className="max-w-prose text-sm leading-relaxed text-white/50">
                Configure your Groq API key and how Floe behaves when your
                system starts.
              </p>
            </div>

            {/* API Key */}
            <div className="flex flex-col gap-3">
              <div className="flex items-baseline justify-between">
                <Label
                  htmlFor="floe-api-key"
                  className="text-xs font-medium uppercase tracking-[0.18em] text-white/45"
                >
                  API key
                </Label>
                <KeyStatusIndicator
                  configured={apiKeyConfigured}
                  isValidating={isValidating}
                />
              </div>
              <Input
                id="floe-api-key"
                type="password"
                placeholder="Enter your API key"
                value={apiKey ?? ""}
                onChange={(e) => {
                  setValidationError(null);
                  setApiKey(e.target.value);
                }}
                onBlur={handleApiKeyBlur}
                className={cn(
                  "h-11 rounded-(--floe-radius-md) px-4 text-sm",
                  validationError &&
                    "border-red-500/60 focus-visible:ring-red-500/40",
                )}
                autoComplete="off"
                spellCheck={false}
              />
              {validationError && (
                <p
                  className="text-xs leading-relaxed text-red-400"
                  role="alert"
                >
                  {validationError}
                </p>
              )}
              {!validationError && (
                <p className="text-xs leading-relaxed text-white/40">
                  Stored securely in your system&rsquo;s keychain and used only
                  for speech-to-text requests. Nothing is logged or shared.
                </p>
              )}
            </div>

            <div className="h-px bg-white/5" />

            {/* Launch on startup */}
            <div className="flex items-center justify-between gap-6">
              <div className="flex flex-col gap-1">
                <Label
                  htmlFor="floe-launch-on-startup"
                  className="text-sm font-medium text-white/85"
                >
                  Launch Floe on system startup
                </Label>
                <p className="text-xs leading-relaxed text-white/40">
                  Start Floe quietly in the background when you sign in. The
                  hotkey will be ready immediately.
                </p>
              </div>
              <Switch
                id="floe-launch-on-startup"
                checked={launchOnStartup}
                onCheckedChange={handleLaunchOnStartupChange}
                className="data-[state=checked]:bg-(--floe-accent)"
              />
            </div>

            <div className="h-px bg-white/5" />

            {/* Skip cleanup */}
            <div className="flex items-center justify-between gap-6">
              <div className="flex flex-col gap-1">
                <Label
                  htmlFor="floe-skip-cleanup"
                  className="text-sm font-medium text-white/85"
                >
                  Skip AI text cleanup (output raw transcript only)
                </Label>
                <p className="text-xs leading-relaxed text-white/40">
                  Bypass the Qwen 3.6 27B cleanup step and paste the raw Whisper
                  transcription directly.
                </p>
              </div>
              <Switch
                id="floe-skip-cleanup"
                checked={skipCleanup}
                onCheckedChange={handleSkipCleanupChange}
                className="data-[state=checked]:bg-(--floe-accent)"
              />
            </div>
          </motion.section>

          {/* ── Audio Card ────────────────────────────────── */}
          <motion.section
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.08, ...cardEnterTransition }}
            className="floe-card-elevated flex flex-col gap-7 p-8"
          >
            <div className="flex flex-col gap-2">
              <span className="floe-eyebrow">Audio</span>
              <h2 className="text-lg font-medium leading-snug text-white/90">
                Microphone
              </h2>
              <p className="max-w-prose text-sm leading-relaxed text-white/50">
                Select the microphone Floe should use for recording.
              </p>
            </div>

            <div className="flex flex-col gap-3">
              <Label
                htmlFor="floe-audio-device"
                className="text-xs font-medium uppercase tracking-[0.18em] text-white/45"
              >
                Input device
              </Label>
              <select
                id="floe-audio-device"
                value={selectedAudioDeviceId ?? ""}
                onChange={(e) => handleAudioDeviceChange(e.target.value)}
                disabled={isLoadingDevices || audioDevices.length === 0}
                className="h-11 rounded-(--floe-radius-md) px-4 text-sm bg-white/5 border border-white/10 text-white/90 focus:outline-none focus:ring-2 focus:ring-(--floe-accent)/50 disabled:opacity-50 disabled:cursor-not-allowed appearance-none"
              >
                {isLoadingDevices ? (
                  <option value="" disabled>
                    Loading devices…
                  </option>
                ) : audioDevices.length === 0 ? (
                  <option value="" disabled>
                    No input devices found
                  </option>
                ) : (
                  audioDevices.map((device: AudioDevice) => (
                    <option key={device.id} value={device.id}>
                      {device.name}
                    </option>
                  ))
                )}
              </select>
              {deviceLoadError && (
                <p
                  role="alert"
                  className="text-xs leading-relaxed text-red-400"
                >
                  {deviceLoadError}
                </p>
              )}
              {!deviceLoadError && selectedAudioDeviceId && (
                <p className="text-xs leading-relaxed text-white/40">
                  Floe will use this microphone for recording.
                </p>
              )}
            </div>
          </motion.section>

          {/* ── Hotkey Card ───────────────────────────────── */}
          <motion.section
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.1, ...cardEnterTransition }}
            className="floe-card-elevated flex flex-col gap-7 p-8"
          >
            <div className="flex flex-col gap-2">
              <span className="floe-eyebrow">Hotkey</span>
              <h2 className="text-lg font-medium leading-snug text-white/90">
                Push-to-talk
              </h2>
              <p className="max-w-prose text-sm leading-relaxed text-white/50">
                Pick a key combination to start and stop recording from anywhere
                on your system.
              </p>
            </div>

            <div className="flex flex-col gap-2">
              <span className="floe-eyebrow">Current combination</span>
              <p
                className={cn(
                  "text-2xl font-medium tracking-tight",
                  hotkey ? "text-white" : "text-white/35",
                )}
              >
                {hotkey ?? "No hotkey set"}
              </p>
            </div>

            <button
              type="button"
              onClick={handleHotkeyPillClick}
              aria-label="Capture new hotkey"
              className={cn(
                "group flex min-h-[88px] w-full flex-col items-center justify-center gap-1 rounded-(--floe-radius-xl) border border-dashed px-6 py-5 text-center transition-all duration-200",
                isHotkeyCaptureActive
                  ? "border-(--floe-accent)/50 bg-(--floe-accent)/[0.08] text-(--floe-accent)"
                  : "border-white/10 bg-white/[0.015] text-white/55 hover:border-white/20 hover:bg-white/[0.03] hover:text-white/80",
              )}
            >
              <span
                className={cn(
                  "text-sm",
                  isHotkeyCaptureActive ? "font-medium" : "font-normal",
                )}
              >
                {isHotkeyCaptureActive
                  ? "Press any key combination\u2026"
                  : "Click to set hotkey"}
              </span>
              {!isHotkeyCaptureActive && (
                <span className="text-[11px] text-white/30">
                  Hold a modifier (Ctrl, Alt, Shift, or Meta) plus a key
                </span>
              )}
            </button>
          </motion.section>

          {/* ── Updates Card ────────────────────────────────── */}
          <motion.section
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.16, ...cardEnterTransition }}
            className="floe-card-elevated flex flex-col gap-7 p-8"
          >
            <div className="flex flex-col gap-2">
              <span className="floe-eyebrow">Updates</span>
              <h2 className="text-lg font-medium leading-snug text-white/90">
                App version
              </h2>
              <p className="max-w-prose text-sm leading-relaxed text-white/50">
                Floe checks for new versions automatically. You can also check
                manually below.
              </p>
            </div>
            <UpdateSection />
          </motion.section>

          {/* ── Hidden Diagnostics Reveal ────────────────── */}
          <div className="flex flex-col items-center gap-4">
            <button
              type="button"
              onClick={() => setShowDiagnostics((prev) => !prev)}
              aria-expanded={showDiagnostics}
              aria-controls="floe-diagnostics-panel"
              className="group flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] font-medium tracking-[0.18em] uppercase text-white/25 transition-colors hover:text-white/55"
            >
              <Activity width={11} height={11} strokeWidth={1.5} />
              <span>Advanced</span>
            </button>

            <AnimatePresence initial={false}>
              {showDiagnostics && (
                <motion.section
                  id="floe-diagnostics-panel"
                  key="diagnostics"
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: "auto", opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                  transition={diagRevealTransition}
                  className="w-full overflow-hidden"
                >
                  <div className="floe-card-elevated flex flex-col gap-5 p-8">
                    <div className="flex flex-col gap-1">
                      <span className="floe-eyebrow">Diagnostics</span>
                      <h3 className="text-sm font-medium text-white/85">
                        Session snapshot
                      </h3>
                      <p className="text-xs leading-relaxed text-white/45">
                        A privacy-safe JSON snapshot of the most recent
                        dictation session. Share with Floe support when
                        reporting an issue.
                      </p>
                    </div>
                    <DiagnosticsSection
                      appVersion={updateInfo?.currentVersion ?? "1.0.0"}
                    />
                  </div>
                </motion.section>
              )}
            </AnimatePresence>
          </div>
        </div>

        {/* ── Footer ─────────────────────────────────────── */}
        <AnimatePresence>
          {saveError && (
            <motion.div
              initial={{ opacity: 0, y: 16 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 16 }}
              transition={{ duration: 0.2 }}
              className="pointer-events-none fixed bottom-20 left-1/2 z-50 -translate-x-1/2 rounded-lg border border-red-500/30 bg-red-500/10 px-5 py-3 text-sm text-red-400 shadow-lg backdrop-blur-sm"
              role="alert"
            >
              {saveError}
            </motion.div>
          )}
        </AnimatePresence>

        <footer className="mt-auto px-8 pb-6 pt-2 text-center">
          <span className="text-[10px] tracking-[0.22em] uppercase text-white/20">
            Version {updateInfo?.currentVersion ?? "1.0.0"}
          </span>
        </footer>
      </main>
    </div>
  );
}

function KeyStatusIndicator({
  configured,
  isValidating,
}: {
  configured: boolean;
  isValidating?: boolean;
}) {
  return (
    <span
      className="flex items-center gap-2 text-[11px] font-medium tracking-[0.04em] text-white/45"
      aria-live="polite"
    >
      {isValidating ? (
        <>
          <span
            aria-hidden
            className="inline-block size-1.5 rounded-full bg-amber-400/70 animate-pulse"
          />
          Validating…
        </>
      ) : (
        <>
          <span
            aria-hidden
            className={cn(
              "inline-block size-1.5 rounded-full transition-colors",
              configured ? "bg-(--floe-accent)" : "bg-white/25",
            )}
          />
          {configured ? "Active" : "Not set"}
        </>
      )}
    </span>
  );
}
