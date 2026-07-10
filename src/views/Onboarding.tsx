import { useCallback, useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  Check,
  KeyRound,
  Keyboard,
  Sparkles,
} from "lucide-react";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import {
  isTauriRuntime,
  saveApiKey,
  validateApiKey,
  setHotkey as setHotkeyBackend,
} from "../lib/tauri";
import {
  errorMessage,
  isKeychainError,
  KEYCHAIN_UNAVAILABLE_MESSAGE,
  logCritical,
} from "../lib/errorLog";
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

type OnboardingStep = "groq" | "hotkey" | "done";

export default function Onboarding() {
  const setupState = useFloeStore((s) => s.deriveSetupState());
  const lastStartupError = useFloeStore((s) => s.lastStartupError);
  const setLastStartupError = useFloeStore((s) => s.setLastStartupError);

  const [step, setStep] = useState<OnboardingStep>(() => {
    if (setupState === "setup_groq") return "groq";
    if (setupState === "setup_hotkey") return "hotkey";
    return "done";
  });

  // Once the user manually navigates (Back/Continue), stop auto-syncing the
  // step to setupState so the user's choice is respected.
  const userNavigatedRef = useRef(false);
  const markUserNavigated = useCallback(() => {
    userNavigatedRef.current = true;
  }, []);

  useEffect(() => {
    if (setupState === "ready") {
      setStep("done");
      return;
    }
    // After user navigation, only "done" may step the user back to an earlier
    // setup state when backend reports a regression.
    if (userNavigatedRef.current) {
      if (step !== "done") return;
    }
    if (setupState === "setup_hotkey" && step === "groq") {
      setStep("hotkey");
      return;
    }
    if (setupState === "setup_groq" && step === "done") {
      setStep("groq");
      return;
    }
    if (setupState === "setup_hotkey" && step === "done") {
      setStep("hotkey");
    }
  }, [setupState, step]);

  const isGroqStep = step === "groq";
  const isHotkeyStep = step === "hotkey";
  const isDoneStep = step === "done";

  const stepIndex = isGroqStep ? 0 : isHotkeyStep ? 1 : 2;

  const handleGroqContinue = useCallback(() => {
    markUserNavigated();
    setStep("hotkey");
  }, [markUserNavigated]);

  const handleHotkeyBack = useCallback(() => {
    markUserNavigated();
    setStep("groq");
  }, [markUserNavigated]);

  const handleHotkeyContinue = useCallback(() => {
    markUserNavigated();
    setStep("done");
  }, [markUserNavigated]);

  return (
    <div className="floe-window flex h-screen flex-col overflow-hidden">
      <div
        className="floe-titlebar flex items-center justify-between"
        data-tauri-drag-region
      >
        <span className="floe-eyebrow">Welcome to Floe</span>
        <span className="floe-eyebrow text-white/30">
          Step {Math.min(stepIndex + 1, 3)} of 3
        </span>
      </div>

      <main className="flex flex-1 flex-col overflow-y-auto">
        <div className="mx-auto flex w-full max-w-[600px] flex-col gap-10 px-8 py-12">
          {lastStartupError && (
            <div
              role="alert"
              className="flex items-start gap-3 rounded-lg border border-red-400/25 bg-red-400/[0.06] px-4 py-3 text-sm text-red-300"
            >
              <AlertTriangle
                width={16}
                height={16}
                strokeWidth={1.5}
                className="mt-0.5 shrink-0 text-red-400/80"
              />
              <div className="flex flex-1 flex-col gap-1">
                <span className="font-medium text-red-300/95">
                  Could not connect to Floe
                </span>
                <span className="text-xs leading-relaxed text-red-300/75">
                  {lastStartupError}
                </span>
                <button
                  type="button"
                  onClick={() => setLastStartupError(null)}
                  className="self-start rounded px-1 py-0.5 text-[11px] font-medium text-red-300/70 transition-colors hover:text-red-300"
                >
                  Dismiss
                </button>
              </div>
            </div>
          )}

          <StepIndicator current={stepIndex} />

          {isGroqStep && <GroqStep onContinue={handleGroqContinue} />}
          {isHotkeyStep && (
            <HotkeyStep
              onBack={handleHotkeyBack}
              onContinue={handleHotkeyContinue}
            />
          )}
          {isDoneStep && <DoneStep />}
        </div>
      </main>
    </div>
  );
}

function StepIndicator({ current }: { current: number }) {
  const labels = ["API key", "Hotkey", "Ready"];
  return (
    <div className="flex items-center gap-3" aria-label="Onboarding progress">
      {labels.map((label, idx) => {
        const isComplete = idx < current;
        const isActive = idx === current;
        return (
          <div key={label} className="flex flex-1 items-center gap-3">
            <div
              className={cn(
                "flex size-7 shrink-0 items-center justify-center rounded-full border text-[11px] font-medium transition-colors",
                isComplete &&
                  "border-(--floe-accent) bg-(--floe-accent) text-(--floe-text-on-accent)",
                isActive &&
                  "border-(--floe-accent) bg-(--floe-accent)/15 text-(--floe-accent)",
                !isComplete &&
                  !isActive &&
                  "border-white/10 bg-transparent text-white/35",
              )}
              aria-current={isActive ? "step" : undefined}
            >
              {isComplete ? <Check width={12} height={12} /> : idx + 1}
            </div>
            <span
              className={cn(
                "text-xs font-medium tracking-wide transition-colors",
                isActive ? "text-white/85" : "text-white/40",
              )}
            >
              {label}
            </span>
            {idx < labels.length - 1 && (
              <span
                aria-hidden
                className={cn(
                  "h-px flex-1 transition-colors",
                  isComplete ? "bg-(--floe-accent)/60" : "bg-white/8",
                )}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

function GroqStep({ onContinue }: { onContinue: () => void }) {
  const apiKey = useFloeStore((s) => s.apiKey);
  const setApiKey = useFloeStore((s) => s.setApiKey);
  const setApiKeyStatus = useFloeStore((s) => s.setApiKeyStatus);
  const reducedMotion = useReducedMotionPreference();

  const [draft, setDraft] = useState<string>(apiKey ?? "");
  const [isValidating, setIsValidating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showSuccess, setShowSuccess] = useState(false);

  const handleContinue = useCallback(async () => {
    const trimmed = draft.trim();
    if (!trimmed) {
      setError("Please enter your Groq API key to continue.");
      return;
    }
    if (!isTauriRuntime()) {
      // Non-Tauri runtime (tests, web preview): accept the key optimistically.
      setApiKey(trimmed);
      setApiKeyStatus(true, "Configured");
      onContinue();
      return;
    }

    setIsValidating(true);
    setError(null);
    try {
      const isValid = await validateApiKey(trimmed);
      if (!isValid) {
        setError("Invalid API key. Please check your Groq console.");
        return;
      }
      await saveApiKey(trimmed);
      setApiKeyStatus(true, "Configured");
      setShowSuccess(true);
      window.setTimeout(() => onContinue(), 450);
    } catch (err) {
      logCritical("onboarding validateAndSaveApiKey", err);
      if (isKeychainError(err)) {
        setError(KEYCHAIN_UNAVAILABLE_MESSAGE);
      } else {
        setError(
          `Could not validate or save your API key: ${errorMessage(err)}. Check your network connection and try again.`,
        );
      }
    } finally {
      setIsValidating(false);
    }
  }, [draft, onContinue, setApiKey, setApiKeyStatus]);

  const cardEnterTransition = reducedMotion
    ? { duration: 0 }
    : { duration: 0.5, ease: [0.16, 1, 0.3, 1] as const };

  return (
    <motion.section
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={cardEnterTransition}
      className="floe-card-elevated flex flex-col gap-7 p-8"
      aria-labelledby="onboarding-groq-title"
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2">
          <span className="flex size-7 items-center justify-center rounded-full bg-(--floe-accent)/15 text-(--floe-accent)">
            <KeyRound width={14} height={14} />
          </span>
          <span className="floe-eyebrow">Step 1</span>
        </div>
        <h2
          id="onboarding-groq-title"
          className="text-lg font-medium leading-snug text-white/90"
        >
          Connect your Groq API key
        </h2>
        <p className="max-w-prose text-sm leading-relaxed text-white/50">
          Floe uses Groq Whisper Turbo for speech-to-text and Groq Qwen 3.6 27B
          for transcript cleanup. You can create a free API key in your{" "}
          <span className="text-white/70">Groq console</span>. The key is stored
          only in your system&rsquo;s keychain.
        </p>
      </div>

      <div className="flex flex-col gap-3">
        <Label
          htmlFor="onboarding-api-key"
          className="text-xs font-medium uppercase tracking-[0.18em] text-white/45"
        >
          Groq API key
        </Label>
        <Input
          id="onboarding-api-key"
          type="password"
          placeholder="gsk_…"
          value={draft}
          onChange={(e) => {
            setError(null);
            setShowSuccess(false);
            const next = e.target.value;
            setDraft(next);
            // Store the trimmed value so Settings never pre-fills an input
            // with leading/trailing whitespace from this draft.
            setApiKey(next.trim());
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void handleContinue();
            }
          }}
          disabled={isValidating || showSuccess}
          className={cn(
            "h-11 rounded-(--floe-radius-md) px-4 text-sm",
            error && "border-red-500/60 focus-visible:ring-red-500/40",
          )}
          autoComplete="off"
          spellCheck={false}
          aria-invalid={Boolean(error)}
          aria-describedby={error ? "onboarding-api-key-error" : undefined}
        />
        {error && (
          <p
            id="onboarding-api-key-error"
            className="text-xs leading-relaxed text-red-400"
            role="alert"
          >
            {error}
          </p>
        )}
        {!error && !showSuccess && (
          <p className="text-xs leading-relaxed text-white/40">
            Stored securely in your system&rsquo;s keychain. Nothing is logged
            or shared.
          </p>
        )}
        {showSuccess && (
          <p
            className="flex items-center gap-2 text-xs leading-relaxed text-(--floe-text-success)"
            role="status"
          >
            <Check width={12} height={12} />
            API key saved. Continuing…
          </p>
        )}
      </div>

      <div className="flex items-center justify-end gap-3">
        <button
          type="button"
          onClick={() => void handleContinue()}
          disabled={isValidating || showSuccess || draft.trim().length === 0}
          className="floe-button-base floe-button-primary flex items-center gap-2 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isValidating ? "Validating…" : "Continue"}
          {!isValidating && <ArrowRight width={14} height={14} />}
        </button>
      </div>
    </motion.section>
  );
}

function HotkeyStep({
  onBack,
  onContinue,
}: {
  onBack: () => void;
  onContinue: () => void;
}) {
  const setHotkey = useFloeStore((s) => s.setHotkey);
  const setHotkeyStatus = useFloeStore((s) => s.setHotkeyStatus);
  const reducedMotion = useReducedMotionPreference();

  const [isCapturing, setIsCapturing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [captured, setCaptured] = useState<string | null>(null);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (
      e.key === "Escape" &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.shiftKey &&
      !e.metaKey
    ) {
      setIsCapturing(false);
      setError(null);
      return;
    }
    const combo = buildHotkeyString(e);
    if (!combo) return;
    setCaptured(combo);
    setIsCapturing(false);
  }, []);

  useEffect(() => {
    if (!isCapturing) return;
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isCapturing, handleKeyDown]);

  const handleSave = useCallback(async () => {
    if (!captured) return;
    if (!isTauriRuntime()) {
      setHotkey(captured);
      setHotkeyStatus(captured, true);
      onContinue();
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      const status = await setHotkeyBackend(captured);
      if (status.error) {
        setError(status.error);
        return;
      }
      setHotkey(status.label || captured);
      setHotkeyStatus(status.label || captured, status.isRegistered);
      onContinue();
    } catch (err) {
      logCritical("onboarding setHotkey", err);
      setError(
        "Could not register the hotkey. Please try a different combination.",
      );
    } finally {
      setIsSaving(false);
    }
  }, [captured, onContinue, setHotkey, setHotkeyStatus]);

  const cardEnterTransition = reducedMotion
    ? { duration: 0 }
    : { duration: 0.5, ease: [0.16, 1, 0.3, 1] as const };

  return (
    <motion.section
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={cardEnterTransition}
      className="floe-card-elevated flex flex-col gap-7 p-8"
      aria-labelledby="onboarding-hotkey-title"
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2">
          <span className="flex size-7 items-center justify-center rounded-full bg-(--floe-accent)/15 text-(--floe-accent)">
            <Keyboard width={14} height={14} />
          </span>
          <span className="floe-eyebrow">Step 2</span>
        </div>
        <h2
          id="onboarding-hotkey-title"
          className="text-lg font-medium leading-snug text-white/90"
        >
          Pick a push-to-talk hotkey
        </h2>
        <p className="max-w-prose text-sm leading-relaxed text-white/50">
          Hold the combination to start recording and release it to stop. Pick
          something you won&rsquo;t press while typing.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <span className="floe-eyebrow">Current combination</span>
        <p
          className={cn(
            "text-2xl font-medium tracking-tight",
            captured ? "text-white" : "text-white/35",
          )}
        >
          {captured ?? "No hotkey set"}
        </p>
      </div>

      <button
        type="button"
        onClick={() => {
          setIsCapturing(true);
          setError(null);
        }}
        disabled={isSaving}
        aria-label="Capture new hotkey"
        className={cn(
          "group flex min-h-[88px] w-full flex-col items-center justify-center gap-1 rounded-(--floe-radius-xl) border border-dashed px-6 py-5 text-center transition-all duration-200",
          isCapturing
            ? "border-(--floe-accent)/50 bg-(--floe-accent)/[0.08] text-(--floe-accent)"
            : "border-white/10 bg-white/[0.015] text-white/55 hover:border-white/20 hover:bg-white/[0.03] hover:text-white/80",
          isSaving && "cursor-not-allowed opacity-50",
        )}
      >
        <span
          className={cn("text-sm", isCapturing ? "font-medium" : "font-normal")}
        >
          {isCapturing
            ? "Press any key combination…"
            : captured
              ? "Click to set a different hotkey"
              : "Click to set hotkey"}
        </span>
        {!isCapturing && (
          <span className="text-[11px] text-white/30">
            Hold a modifier (Ctrl, Alt, Shift, or Meta) plus a key
          </span>
        )}
      </button>

      {error && (
        <p className="text-xs leading-relaxed text-red-400" role="alert">
          {error}
        </p>
      )}

      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={onBack}
          disabled={isSaving}
          className="floe-button-base flex items-center gap-2 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <ArrowLeft width={14} height={14} />
          Back
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={isSaving || !captured}
          className="floe-button-base floe-button-primary flex items-center gap-2 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? "Saving…" : "Continue"}
          {!isSaving && <ArrowRight width={14} height={14} />}
        </button>
      </div>
    </motion.section>
  );
}

function DoneStep() {
  const setupState = useFloeStore((s) => s.deriveSetupState());
  const reducedMotion = useReducedMotionPreference();
  const [showSettings, setShowSettings] = useState(false);
  const settingsRef = useRef(false);
  settingsRef.current = showSettings;

  useEffect(() => {
    if (setupState !== "ready") return;
    if (settingsRef.current) return;
    const id = window.setTimeout(() => setShowSettings(true), 1200);
    return () => window.clearTimeout(id);
  }, [setupState]);

  const cardEnterTransition = reducedMotion
    ? { duration: 0 }
    : { duration: 0.5, ease: [0.16, 1, 0.3, 1] as const };

  return (
    <motion.section
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={cardEnterTransition}
      className="floe-card-elevated flex flex-col items-center gap-6 p-10 text-center"
      aria-labelledby="onboarding-done-title"
    >
      <div className="flex size-12 items-center justify-center rounded-full bg-(--floe-accent)/15 text-(--floe-accent)">
        <Sparkles width={20} height={20} />
      </div>
      <div className="flex flex-col gap-2">
        <h2
          id="onboarding-done-title"
          className="text-lg font-medium leading-snug text-white/90"
        >
          You&rsquo;re all set
        </h2>
        <p className="max-w-prose text-sm leading-relaxed text-white/50">
          Hold your push-to-talk hotkey anywhere on your computer to dictate.
          Floe transcribes your speech, polishes it, and pastes the result where
          your cursor is.
        </p>
      </div>
      {showSettings && (
        <p className="text-xs leading-relaxed text-white/35" role="status">
          Opening settings…
        </p>
      )}
    </motion.section>
  );
}
