import { useEffect } from "react";
import { motion } from "framer-motion";
import { Mic, Loader2 } from "lucide-react";
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
};

const SUBTITLES: Record<string, string> = {
  idle: "Press your global hotkey to start",
  recording: "Press again or click to stop",
  processing: "Result will be copied to clipboard",
};

const ARIA_LABELS: Record<string, string> = {
  idle: "Floe is ready",
  recording: "Recording",
  processing: "Processing dictation",
};

// ── Framer Motion variants ─────────────────────────────────

const variants = {
  idle: { scale: 1 },
  recording: { scale: 1.02 },
  processing: { scale: 1 },
};

const defaultTransition = { duration: 0.15, ease: [0.16, 1, 0.3, 1] };

// ── Component ──────────────────────────────────────────────

export default function FloatingRecorderOverlay() {
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

  useEffect(() => {
    if (status !== "recording" || recordingStartedAt === null) return;
    const id = setInterval(() => tickRecording(Date.now()), 100);
    return () => clearInterval(id);
  }, [status, recordingStartedAt, tickRecording]);

  const handleClick = () => {
    if (isIdle()) {
      startRecording();
    } else if (isRecording()) {
      stopRecordingAndProcess();
    }
  };

  return (
    <motion.div
      role="status"
      aria-live="polite"
      aria-label={ARIA_LABELS[status]}
      className="fixed bottom-6 left-1/2 z-50 flex h-[52px] w-[280px] -translate-x-1/2 items-center justify-between gap-2 rounded-[var(--floe-radius-full)] border bg-[#0A0A0A]/90 backdrop-blur-md px-[10px] py-2 shadow-[var(--floe-shadow-soft)]"
      style={{
        borderColor: isRecording()
          ? "var(--floe-border-focus)"
          : "var(--floe-border-subtle)",
        boxShadow: "inset 0 1px 0 rgba(255, 255, 255, 0.05)",
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
        {isIdle() && (
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
      </div>

      {/* ── Right: timer / badge ────────────────────────── */}
      <div className="relative z-10 shrink-0 pr-1">
        {isIdle() && hotkey && (
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

        {isIdle() && !hotkey && (
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
      </div>
    </motion.div>
  );
}
