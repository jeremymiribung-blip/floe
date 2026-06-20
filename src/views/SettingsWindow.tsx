import { useEffect, useCallback, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Activity, X } from "lucide-react";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Switch } from "../components/ui/switch";
import { DiagnosticsSection } from "../components/DiagnosticsSection";
import {
  isTauriRuntime,
  saveApiKey,
  setHotkey as setHotkeyBackend,
  setStartAtLoginEnabled,
} from "../lib/tauri";
import useFloeStore from "../stores/useFloeStore";
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

const APP_VERSION = "1.0.0";

export default function SettingsWindow({ onClose }: SettingsWindowProps) {
  const apiKey = useFloeStore((s) => s.apiKey);
  const hotkey = useFloeStore((s) => s.hotkey);
  const isHotkeyCaptureActive = useFloeStore((s) => s.isHotkeyCaptureActive);
  const launchOnStartup = useFloeStore((s) => s.launchOnStartup);
  const apiKeyConfigured = useFloeStore((s) => s.apiKeyConfigured);
  const setApiKey = useFloeStore((s) => s.setApiKey);
  const setHotkey = useFloeStore((s) => s.setHotkey);
  const setLaunchOnStartup = useFloeStore((s) => s.setLaunchOnStartup);
  const startHotkeyCapture = useFloeStore((s) => s.startHotkeyCapture);
  const stopHotkeyCapture = useFloeStore((s) => s.stopHotkeyCapture);
  const closeSettings = useFloeStore((s) => s.closeSettings);

  const reducedMotion = useReducedMotionPreference();
  const [showDiagnostics, setShowDiagnostics] = useState(false);

  const apiKeyRef = useRef(apiKey);
  apiKeyRef.current = apiKey;

  const handleClose = () => {
    saveCurrentApiKey();
    closeSettings();
    onClose?.();
  };

  const saveCurrentApiKey = useCallback(() => {
    const key = apiKeyRef.current;
    if (!isTauriRuntime() || !key) return;
    saveApiKey(key).catch((err) => console.error("saveApiKey failed:", err));
  }, []);

  const handleApiKeyBlur = useCallback(() => {
    saveCurrentApiKey();
  }, [saveCurrentApiKey]);

  // ── Keydown listener for hotkey capture ────────────────
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const combo = buildHotkeyString(e);
      if (!combo) return;

      setHotkey(combo);
      stopHotkeyCapture();

      if (isTauriRuntime()) {
        setHotkeyBackend(combo).catch((err) =>
          console.error("setHotkeyBackend failed:", err),
        );
      }
    },
    [setHotkey, stopHotkeyCapture],
  );

  useEffect(() => {
    if (!isHotkeyCaptureActive) return;
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isHotkeyCaptureActive, handleKeyDown]);

  const handleHotkeyPillClick = () => {
    if (!isHotkeyCaptureActive) startHotkeyCapture();
  };

  const handleLaunchOnStartupChange = useCallback(
    (checked: boolean) => {
      setLaunchOnStartup(checked);
      if (isTauriRuntime()) {
        setStartAtLoginEnabled(checked).catch((err) =>
          console.error("setStartAtLogin failed:", err),
        );
      }
    },
    [setLaunchOnStartup],
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
                <KeyStatusIndicator configured={apiKeyConfigured} />
              </div>
              <Input
                id="floe-api-key"
                type="password"
                placeholder="Enter your API key"
                value={apiKey ?? ""}
                onChange={(e) => setApiKey(e.target.value)}
                onBlur={handleApiKeyBlur}
                className="h-11 rounded-(--floe-radius-md) px-4 text-sm"
                autoComplete="off"
                spellCheck={false}
              />
              <p className="text-xs leading-relaxed text-white/40">
                Stored securely in your system&rsquo;s keychain and used only
                for speech-to-text requests. Nothing is logged or shared.
              </p>
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
                    <DiagnosticsSection appVersion={APP_VERSION} />
                  </div>
                </motion.section>
              )}
            </AnimatePresence>
          </div>
        </div>

        {/* ── Footer ─────────────────────────────────────── */}
        <footer className="mt-auto px-8 pb-6 pt-2 text-center">
          <span className="text-[10px] tracking-[0.22em] uppercase text-white/20">
            Version {APP_VERSION}
          </span>
        </footer>
      </main>
    </div>
  );
}

function KeyStatusIndicator({ configured }: { configured: boolean }) {
  return (
    <span
      className="flex items-center gap-2 text-[11px] font-medium tracking-[0.04em] text-white/45"
      aria-live="polite"
    >
      <span
        aria-hidden
        className={cn(
          "inline-block size-1.5 rounded-full transition-colors",
          configured ? "bg-(--floe-accent)" : "bg-white/25",
        )}
      />
      {configured ? "Active" : "Not set"}
    </span>
  );
}
