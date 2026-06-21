import { useEffect } from "react";
import { motion } from "framer-motion";
import { Mic, Loader2, AlertCircle, FileText } from "lucide-react";
import type { AppState } from "../../types/app";
import useFloeStore from "../../stores/useFloeStore";
import { useReducedMotionPreference } from "../../hooks/useReducedMotionPreference";

// ── Helpers ────────────────────────────────────────────────

function formatTimer(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

// ── Static labels per state ────────────────────────────────

const TITLES: Record<string, string> = {
  idle: "Floe is ready",
  recording: "Listening\u2026",
  processing: "Transcribing\u2026",
  preview: "Preview transcript",
  error: "Recording failed",
};

const SUBTITLES: Record<string, string> = {
  idle: "Press your global hotkey to start",
  recording: "Press again or click to stop",
  processing: "Result will be copied to clipboard",
  preview: "Press Enter to insert, Esc to cancel",
  error: "Click to dismiss",
};

const ARIA_LABELS: Record<string, string> = {
  idle: "Floe is ready",
  recording: "Recording",
  processing: "Processing dictation",
  preview: "Preview transcript",
  error: "Recording failed",
};

// ── Framer Motion variants ─────────────────────────────────

const variants = {
  idle: { scale: 1 },
  recording: { scale: 1.02 },
  processing: { scale: 1 },
  preview: { scale: 1 },
  error: { scale: 1 },
};

const defaultTransition = { duration: 0.15, ease: [0.16, 1, 0.3, 1] };

// ── Component ──────────────────────────────────────────────

interface FloatingRecorderOverlayProps {
  error: string | null;
  appState: AppState;
  transcript: string | null;
  onConfirm: () => void;
  onDiscard: () => void;
}

export default function FloatingRecorderOverlay({
  error,
  appState,
  transcript,
  onConfirm,
  onDiscard,
}: FloatingRecorderOverlayProps) {
  const status = useFloeStore((s) => s.status);
  const recordingDurationMs = useFloeStore((s) => s.recordingDurationMs);
  const hotkey = useFloeStore((s) => s.hotkey);
  const isIdle = useFloeStore((s) => s.isIdle);
  const isRecording = useFloeStore((s) => s.isRecording);
  const isProcessing = useFloeStore((s) => s.isProcessing);
  const startRecording = useFloeStore((s) => s.startRecording);
  const stopRecordingAndProcess = useFloeStore(
    (s) => s.stopRecordingAndProcess,
  );
  const tickRecording = useFloeStore((s) => s.tickRecording);
  const recordingStartedAt = useFloeStore((s) => s.recordingStartedAt);
  const reduced = useReducedMotionPreference();
  const isError = Boolean(error);
  const isPreview = appState === "preview";

  useEffect(() => {
    if (status !== "recording" || recordingStartedAt === null) return;
    const id = setInterval(() => tickRecording(Date.now()), 100);
    return () => clearInterval(id);
  }, [status, recordingStartedAt, tickRecording]);

  // ── Keyboard listeners for preview mode ─────────────────
  useEffect(() => {
    if (!isPreview) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        onConfirm();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onDiscard();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isPreview, onConfirm, onDiscard]);

  const handleClick = () => {
    if (isPreview) {
      // Clicking the preview overlay confirms the transcript
      onConfirm();
      return;
    }
    if (isError) {
      // Error state: click dismisses the overlay (handled by parent via appState)
      return;
    }
    if (isIdle()) {
      startRecording();
    } else if (isRecording()) {
      stopRecordingAndProcess();
    }
  };

  // ── Preview rendering: expanded bubble with transcript ──
  if (isPreview) {
    return (
      <motion.div
        role="dialog"
        aria-modal="true"
        aria-label={ARIA_LABELS.preview}
        className="fixed bottom-6 left-1/2 z-50 -translate-x-1/2 cursor-pointer select-none"
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={reduced ? { duration: 0 } : defaultTransition}
        onClick={handleClick}
      >
        <div
          className="flex flex-col gap-2 rounded-[var(--floe-radius-xl)] border px-4 py-3 shadow-[var(--floe-shadow-soft)]"
          style={{
            backgroundColor: "rgba(10, 10, 10, 0.94)",
            borderColor: "var(--floe-border-subtle)",
            backdropFilter: "blur(14px)",
            WebkitBackdropFilter: "blur(14px)",
            minWidth: "300px",
            maxWidth: "480px",
          }}
        >
          {/* ── Header row ──────────────────────────────────── */}
          <div className="flex items-center gap-2">
            <div
              className="flex size-[22px] shrink-0 items-center justify-center rounded-full"
              style={{
                backgroundColor: "var(--floe-bg-elevated)",
                border: "1px solid var(--floe-border-strong)",
              }}
            >
              <FileText
                className="size-[14px]"
                style={{ color: "var(--floe-accent)" }}
              />
            </div>
            <span
              className="text-[12px] font-medium leading-tight"
              style={{ color: "var(--floe-text-primary)" }}
            >
              {TITLES.preview}
            </span>
          </div>

          {/* ── Transcript text ─────────────────────────────── */}
          <div
            className="rounded-[var(--floe-radius-md)] px-3 py-2 text-[12px] leading-relaxed"
            style={{
              backgroundColor: "var(--floe-bg-subtle)",
              border: "1px solid var(--floe-border-subtle)",
              color: "var(--floe-text-primary)",
              maxHeight: "120px",
              overflowY: "auto",
              wordBreak: "break-word",
            }}
          >
            {transcript || ""}
          </div>

          {/* ── Keyboard hint ───────────────────────────────── */}
          <div className="flex items-center gap-3">
            <div
              className="flex items-center gap-1.5 rounded-full px-2.5 py-1"
              style={{
                backgroundColor: "var(--floe-bg-subtle)",
                border: "1px solid var(--floe-border-subtle)",
              }}
            >
              <span
                className="text-[10px] font-mono leading-none tracking-wide font-semibold"
                style={{ color: "var(--floe-accent)" }}
              >
                Enter
              </span>
              <span
                className="text-[10px] leading-none"
                style={{ color: "var(--floe-text-muted)" }}
              >
                insert
              </span>
            </div>
            <div
              className="flex items-center gap-1.5 rounded-full px-2.5 py-1"
              style={{
                backgroundColor: "var(--floe-bg-subtle)",
                border: "1px solid var(--floe-border-subtle)",
              }}
            >
              <span
                className="text-[10px] font-mono leading-none tracking-wide font-semibold"
                style={{ color: "var(--floe-text-secondary)" }}
              >
                Esc
              </span>
              <span
                className="text-[10px] leading-none"
                style={{ color: "var(--floe-text-muted)" }}
              >
                cancel
              </span>
            </div>
            <div className="ml-auto">
              <span
                className="text-[10px] leading-none"
                style={{ color: "var(--floe-text-muted)" }}
              >
                Click to insert
              </span>
            </div>
          </div>
        </div>
      </motion.div>
    );
  }

  return (
    <motion.div
      role="status"
      aria-live="polite"
      aria-label={ARIA_LABELS[status]}
      className="fixed bottom-6 left-1/2 z-50 flex h-[52px] w-[280px] -translate-x-1/2 items-center justify-between gap-2 rounded-[var(--floe-radius-full)] border bg-[#0A0A0A]/90 backdrop-blur-md px-[10px] py-2 shadow-[var(--floe-shadow-soft)]"
      style={{
        borderColor: isError
          ? "var(--floe-error-border)"
          : isRecording()
          ? "var(--floe-border-focus)"
          : "var(--floe-border-subtle)",
        boxShadow: isError
          ? "inset 0 1px 0 rgba(255, 255, 255, 0.05), 0 0 0 1px var(--floe-error-border)"
          : "inset 0 1px 0 rgba(255, 255, 255, 0.05)",
      }}
      initial={false}
      animate={status}
      variants={variants}
      transition={reduced ? { duration: 0 } : defaultTransition}
      onClick={handleClick}
    >
      {/* ── Recording accent glow (behind content) ──────────── */}
      {isRecording() && (
        <motion.div className="absolute inset-0 z-0 rounded-[var(--floe-radius-full)] bg-[#52EEE5] opacity-35 blur-md" />
      )}

      {/* ── Left: status indicator (22x22) ──────────────────── */}
      <div className="relative z-10 flex shrink-0 size-[22px] items-center justify-center">
        {isError && (
          <div
            className="flex size-[22px] items-center justify-center rounded-full"
            style={{ backgroundColor: "var(--floe-error-soft)" }}
          >
            <AlertCircle
              className="size-[14px]"
              style={{ color: "var(--floe-error)" }}
            />
          </div>
        )}

        {isIdle() && !isError && (
          <div
            className="flex size-[22px] items-center justify-center rounded-full"
            style={{
              border: "1px solid var(--floe-border-strong)",
            }}
          >
            <Mic
              className="size-[14px]"
              style={{ color: "var(--floe-text-secondary)" }}
            />
          </div>
        )}

        {isRecording() && (
          <div
            className="flex size-[22px] items-center justify-center rounded-full"
            style={{ backgroundColor: "var(--floe-accent)" }}
          >
            <Mic
              className="size-[14px]"
              style={{ color: "var(--floe-text-on-accent)" }}
            />
          </div>
        )}

        {isProcessing() && (
          <div
            className="flex size-[22px] items-center justify-center rounded-full"
            style={{
              backgroundColor: "var(--floe-bg-elevated)",
              border: "1px solid var(--floe-border-strong)",
            }}
          >
            <Loader2
              className="size-[14px] animate-spin"
              style={{ color: "var(--floe-accent)" }}
            />
          </div>
        )}
      </div>

      {/* ── Center: text block ──────────────────────────── */}
      <div className="relative z-10 flex min-w-0 flex-1 flex-col leading-tight px-[10px]">
        <span
          className="truncate text-[12px] font-medium"
          style={{ color: "var(--floe-text-primary)" }}
        >
          {TITLES[status]}
        </span>
        <span
          className="truncate text-[10px] tracking-wide"
          style={{ color: "var(--floe-text-muted)" }}
        >
          {SUBTITLES[status]}
        </span>
        {isError && error && (
          <span
            className="truncate text-[10px] tracking-wide mt-0.5"
            style={{ color: "var(--floe-error)" }}
          >
            {error}
          </span>
        )}
      </div>

      {/* ── Right: timer / badge ────────────────────────── */}
      <div className="relative z-10 shrink-0 pr-1">
        {isIdle() && !isError && hotkey && (
          <div
            className="flex h-[28px] items-center rounded-full px-2.5"
            style={{
              backgroundColor: "var(--floe-bg-subtle)",
              border: "1px solid var(--floe-border-subtle)",
            }}
          >
            <span
              className="text-[10px] font-mono leading-none tracking-wide"
              style={{ color: "var(--floe-text-secondary)" }}
            >
              {hotkey}
            </span>
          </div>
        )}

        {isIdle() && !isError && !hotkey && (
          <div
            className="flex h-[28px] items-center rounded-full px-2.5"
            style={{
              backgroundColor: "var(--floe-bg-subtle)",
              border: "1px solid var(--floe-border-subtle)",
            }}
          >
            <span
              className="text-[10px] font-mono leading-none tracking-wide"
              style={{ color: "var(--floe-text-muted)" }}
            >
              Set hotkey
            </span>
          </div>
        )}

        {isRecording() && (
          <div
            className="flex h-[28px] items-center rounded-full px-2.5"
            style={{ backgroundColor: "var(--floe-accent-soft)" }}
          >
            <span
              className="text-[10px] font-mono leading-none tabular-nums tracking-wide"
              style={{ color: "var(--floe-accent)" }}
            >
              {formatTimer(recordingDurationMs)}
            </span>
          </div>
        )}

        {isProcessing() && (
          <div
            className="flex h-[28px] items-center rounded-full px-2.5"
            style={{
              backgroundColor: "var(--floe-bg-subtle)",
              border: "1px solid var(--floe-border-subtle)",
            }}
          >
            <span
              className="text-[10px] font-mono leading-none tracking-wide"
              style={{ color: "var(--floe-text-secondary)" }}
            >
              Working
            </span>
          </div>
        )}

        {isError && (
          <div
            className="flex h-[28px] items-center rounded-full px-2.5"
            style={{
              backgroundColor: "var(--floe-error-soft)",
              border: "1px solid var(--floe-error-border)",
            }}
          >
            <span
              className="text-[10px] font-mono leading-none tracking-wide"
              style={{ color: "var(--floe-error)" }}
            >
              Error
            </span>
          </div>
        )}
      </div>
    </motion.div>
  );
}