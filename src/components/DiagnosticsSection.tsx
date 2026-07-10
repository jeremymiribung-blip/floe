import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Copy, RefreshCw } from "lucide-react";
import {
  diagnosticsReportToJson,
  copyDiagnosticsReportToClipboard,
  emptyDiagnosticsReport,
  assertDiagnosticsReportSafe,
} from "../lib/diagnosticsReport";
import { logRecoverable, errorMessage } from "../lib/errorLog";
import {
  getDiagnosticsReport,
  isTauriRuntime,
  type DiagnosticsReport,
} from "../lib/tauri";

interface DiagnosticsSectionProps {
  appVersion: string;
}

const PREVIEW_LINE_LIMIT = 80;

export function DiagnosticsSection({ appVersion }: DiagnosticsSectionProps) {
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [copyStatus, setCopyStatus] = useState<"idle" | "copied" | "failed">(
    "idle",
  );
  const [expanded, setExpanded] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadReport = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      if (!isTauriRuntime()) {
        setReport(
          emptyDiagnosticsReport("Floe", appVersion, new Date().toISOString()),
        );
        return;
      }
      const next = await getDiagnosticsReport();
      assertDiagnosticsReportSafe(next);
      setReport(next);
    } catch (caught) {
      const message = errorMessage(caught);
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [appVersion]);

  useEffect(() => {
    void loadReport();
  }, [loadReport]);

  useEffect(() => {
    return () => {
      if (copyTimerRef.current !== null) {
        clearTimeout(copyTimerRef.current);
      }
    };
  }, []);

  const handleCopy = useCallback(async () => {
    if (!report) {
      return;
    }
    try {
      await copyDiagnosticsReportToClipboard(report);
      setCopyStatus("copied");
    } catch (err) {
      logRecoverable("diagnostics copy to clipboard", err);
      setCopyStatus("failed");
    } finally {
      if (copyTimerRef.current !== null) {
        clearTimeout(copyTimerRef.current);
      }
      copyTimerRef.current = setTimeout(() => {
        setCopyStatus("idle");
        copyTimerRef.current = null;
      }, 2000);
    }
  }, [report]);

  const json = report ? diagnosticsReportToJson(report) : "";
  const previewLines = json.split("\n");
  const shouldTruncate = previewLines.length > PREVIEW_LINE_LIMIT;
  const displayed = expanded
    ? json
    : shouldTruncate
      ? previewLines.slice(0, PREVIEW_LINE_LIMIT).join("\n")
      : json;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-col gap-1">
        <label
          className="text-[11px] tracking-wide"
          style={{ color: "var(--floe-text-muted)" }}
        >
          Diagnostics
        </label>
        <p
          className="text-[10px] tracking-wide"
          style={{ color: "var(--floe-text-muted)" }}
        >
          Copy a detailed, privacy-safe JSON snapshot of the most recent
          dictation session. Share it with Floe support when reporting a
          problem.
        </p>
      </div>

      <div className="flex items-center gap-2">
        <button
          type="button"
          className="flex h-7 items-center gap-1.5 rounded-[var(--floe-radius-sm)] border px-3 text-[12px] font-medium transition-colors disabled:opacity-50 bg-[#0A0A0A]/90 backdrop-blur-md"
          style={{
            borderColor: "var(--floe-border-subtle)",
            color: "var(--floe-text-primary)",
            boxShadow: "inset 0 1px 0 rgba(255, 255, 255, 0.05)",
          }}
          onClick={handleCopy}
          disabled={!report}
          aria-label="Copy diagnostics JSON"
        >
          <Copy width={12} height={12} />
          Copy diagnostics JSON
        </button>
        <button
          type="button"
          className="flex h-7 w-7 items-center justify-center rounded-[var(--floe-radius-sm)] border transition-colors disabled:opacity-50 bg-[#0A0A0A]/90 backdrop-blur-md"
          style={{
            borderColor: "var(--floe-border-subtle)",
            color: "var(--floe-text-secondary)",
          }}
          onClick={() => {
            void loadReport();
          }}
          disabled={isLoading}
          aria-label="Refresh diagnostics"
        >
          <RefreshCw
            width={12}
            height={12}
            className={isLoading ? "animate-spin" : undefined}
          />
        </button>
        {copyStatus === "copied" ? (
          <span
            className="text-[10px] tracking-wide"
            style={{ color: "var(--floe-text-secondary)" }}
          >
            Copied
          </span>
        ) : copyStatus === "failed" ? (
          <span
            className="text-[10px] tracking-wide"
            style={{ color: "var(--floe-text-secondary)" }}
          >
            Copy failed
          </span>
        ) : null}
      </div>

      {error ? (
        <p
          className="text-[10px] tracking-wide"
          style={{ color: "var(--floe-text-secondary)" }}
        >
          {error}
        </p>
      ) : null}

      {report ? (
        <div
          className="rounded-[var(--floe-radius-sm)] border"
          style={{
            borderColor: "var(--floe-border-subtle)",
            backgroundColor: "rgba(10,10,10,0.85)",
          }}
        >
          <button
            type="button"
            className="flex w-full items-center justify-between px-2 py-1 text-left"
            onClick={() => setExpanded((prev) => !prev)}
            aria-expanded={expanded}
          >
            <span
              className="text-[10px] tracking-wide"
              style={{ color: "var(--floe-text-muted)" }}
            >
              {expanded ? "Hide JSON" : "Preview JSON"}
            </span>
            {expanded ? (
              <ChevronDown
                width={10}
                height={10}
                style={{ color: "var(--floe-text-muted)" }}
              />
            ) : (
              <ChevronRight
                width={10}
                height={10}
                style={{ color: "var(--floe-text-muted)" }}
              />
            )}
          </button>
          {expanded ? (
            <pre
              className="max-h-[260px] overflow-auto px-2 pb-2 font-mono text-[10px] leading-[1.4]"
              style={{ color: "var(--floe-text-secondary)" }}
            >
              {displayed}
              {shouldTruncate && !expanded ? "\n…" : ""}
            </pre>
          ) : null}
        </div>
      ) : (
        <p
          className="text-[10px] tracking-wide"
          style={{ color: "var(--floe-text-muted)" }}
        >
          {isLoading ? "Loading diagnostics…" : "No diagnostics available."}
        </p>
      )}
    </div>
  );
}
