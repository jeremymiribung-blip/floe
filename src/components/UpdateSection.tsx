import { useCallback } from "react";
import { RefreshCw, Download, RotateCw, XCircle } from "lucide-react";
import {
  isTauriRuntime,
  checkForUpdate,
  downloadUpdate,
  installUpdate,
  resetUpdateState,
} from "../lib/tauri";
import { logCritical, errorMessage } from "../lib/errorLog";
import useFloeStore from "../stores/useFloeStore";
import { cn } from "../lib/utils";
import type { UpdateInfo } from "../types/app";

// ── User-friendly error summaries per error code ──────────────────────────

const ERROR_SUMMARIES: Record<string, { title: string; hint: string }> = {
  gitHubApiUnreachable: {
    title: "Could not check for updates",
    hint: "Check your internet connection and try again.",
  },
  releaseNotFound: {
    title: "Update source not found",
    hint: "The GitHub repository or releases page is unavailable.",
  },
  malformedReleaseData: {
    title: "Update data error",
    hint: "The release information from GitHub could not be read.",
  },
  noCompatibleAsset: {
    title: "No update for this platform",
    hint: "No installer was found for your operating system.",
  },
  versionParseFailed: {
    title: "Version comparison error",
    hint: "A version number could not be parsed.",
  },
  downloadFailed: {
    title: "Download failed",
    hint: "The update could not be downloaded. Check your connection and try again.",
  },
  checksumMismatch: {
    title: "Download corrupted",
    hint: "The downloaded file did not pass integrity verification. Please retry.",
  },
  installFailed: {
    title: "Installation failed",
    hint: "The installer could not be launched. Try downloading again.",
  },
  alreadyChecking: {
    title: "Already in progress",
    hint: "An update check or download is already running.",
  },
};

/** Map a raw error message + error code to a friendly display object. */
function friendlyError(
  rawMessage: string | null | undefined,
  code?: string,
): { title: string; detail: string } {
  if (!rawMessage) {
    return { title: "Update error", detail: "An unknown error occurred." };
  }

  // If we have a known error code, use the friendly summary
  if (code) {
    const summary = ERROR_SUMMARIES[code];
    if (summary) {
      return { title: summary.title, detail: `${summary.hint}\n${rawMessage}` };
    }
  }

  // Detect common scenarios from the message text
  if (rawMessage.includes("rate limit") || rawMessage.includes("403")) {
    return {
      title: "GitHub rate limit hit",
      detail: "Please wait a moment before checking again.",
    };
  }
  if (rawMessage.includes("reach GitHub") || rawMessage.includes("internet")) {
    return {
      title: "Could not check for updates",
      detail: "Check your internet connection and try again.",
    };
  }
  if (rawMessage.includes("404") || rawMessage.includes("not found")) {
    return {
      title: "Update not found",
      detail: "The update source was not found on GitHub.",
    };
  }

  // Fallback: show the raw message
  return { title: "Update error", detail: rawMessage };
}

/** Extract message + code from a thrown backend error. */
function readBackendError(err: unknown): { message: string; code?: string } {
  if (err && typeof err === "object" && "message" in err) {
    const payload = err as { message: unknown; code?: unknown };
    const message =
      typeof payload.message === "string"
        ? payload.message
        : errorMessage(err);
    const code = typeof payload.code === "string" ? payload.code : undefined;
    return { message, code };
  }
  return { message: errorMessage(err) };
}

// ── Helpers ───────────────────────────────────────────────────────────────

function errorUpdateInfo(
  currentVersion: string | undefined,
  latestVersion: string | null | undefined,
  fallbackMessage: string,
): UpdateInfo {
  return {
    currentVersion: currentVersion ?? "1.0.0",
    latestVersion: latestVersion ?? null,
    status: "error",
    downloadProgress: 0,
    lastCheckResult: null,
    errorMessage: fallbackMessage,
  };
}

// ── Component ─────────────────────────────────────────────────────────────

export default function UpdateSection() {
  const updateInfo = useFloeStore((s) => s.updateInfo);
  const setUpdateInfo = useFloeStore((s) => s.setUpdateInfo);
  const updateCheckInProgress = useFloeStore((s) => s.updateCheckInProgress);
  const setUpdateCheckInProgress = useFloeStore(
    (s) => s.setUpdateCheckInProgress,
  );

  const handleCheckForUpdate = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setUpdateCheckInProgress(true);
    try {
      const info = await checkForUpdate();
      setUpdateInfo(info);
    } catch (err: unknown) {
      logCritical("update checkForUpdate", err);
      const { message } = readBackendError(err);
      setUpdateInfo(
        errorUpdateInfo(updateInfo?.currentVersion, null, message || "Update check failed."),
      );
    }
  }, [setUpdateInfo, setUpdateCheckInProgress, updateInfo?.currentVersion]);

  const handleDownloadUpdate = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const info = await downloadUpdate();
      setUpdateInfo(info);
    } catch (err: unknown) {
      logCritical("update downloadUpdate", err);
      const { message } = readBackendError(err);
      setUpdateInfo(
        errorUpdateInfo(updateInfo?.currentVersion, updateInfo?.latestVersion, message || "Download failed."),
      );
    }
  }, [setUpdateInfo, updateInfo]);

  const handleInstallUpdate = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      await installUpdate();
    } catch (err: unknown) {
      logCritical("update installUpdate", err);
      const { message } = readBackendError(err);
      setUpdateInfo(
        errorUpdateInfo(updateInfo?.currentVersion, updateInfo?.latestVersion, message || "Installation failed."),
      );
    }
  }, [setUpdateInfo, updateInfo?.currentVersion, updateInfo?.latestVersion]);

  const handleDismiss = useCallback(async () => {
    if (!isTauriRuntime()) {
      setUpdateInfo(null);
      return;
    }
    try {
      await resetUpdateState();
      setUpdateInfo(null);
    } catch (err: unknown) {
      logCritical("update resetUpdateState", err);
      const { message } = readBackendError(err);
      setUpdateInfo(
        errorUpdateInfo(updateInfo?.currentVersion, updateInfo?.latestVersion, message || "Could not dismiss update."),
      );
    }
  }, [setUpdateInfo, updateInfo?.currentVersion, updateInfo?.latestVersion]);

  const status = updateInfo?.status;

  // ── Derive friendly error from UpdateInfo ───────────────────────
  const friendly =
    status === "error" && updateInfo?.errorMessage
      ? friendlyError(updateInfo.errorMessage)
      : null;

  return (
    <div className="flex flex-col gap-4">
      {/* Current and latest version */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-sm text-white/65">Current version</span>
          <span className="text-sm font-medium text-white/90">
            v{updateInfo?.currentVersion ?? "1.0.0"}
          </span>
        </div>
        {status === "available" ||
        status === "downloading" ||
        status === "downloaded" ? (
          <div className="flex items-center justify-between">
            <span className="text-sm text-(--floe-accent)">Available</span>
            <span className="text-sm font-medium text-(--floe-accent)">
              v{updateInfo?.latestVersion ?? "?"}
            </span>
          </div>
        ) : status === "no_update" ? (
          <div className="flex items-center justify-between">
            <span className="text-sm text-white/40">Latest</span>
            <span className="text-sm text-white/40">
              v{updateInfo?.latestVersion ?? "1.0.0"}
            </span>
          </div>
        ) : null}
      </div>

      {/* Error display */}
      {status === "error" && friendly && (
        <div className="flex flex-col gap-1.5 rounded-md border border-red-400/20 bg-red-400/5 px-3 py-2.5">
          <div className="flex items-start gap-2">
            <XCircle
              width={14}
              height={14}
              strokeWidth={1.5}
              className="mt-0.5 shrink-0 text-red-400/70"
            />
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium text-red-400/90">
                {friendly.title}
              </span>
              <span className="text-[11px] leading-relaxed text-red-400/60">
                {friendly.detail}
              </span>
            </div>
          </div>
          <button
            type="button"
            onClick={handleDismiss}
            className="self-end rounded px-2 py-0.5 text-[11px] font-medium text-red-400/60 transition-colors hover:text-red-400/90"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Success / info message */}
      {updateInfo?.lastCheckResult && status !== "error" && (
        <p className="text-xs leading-relaxed text-white/40">
          {updateInfo.lastCheckResult}
        </p>
      )}

      {/* Progress bar */}
      {status === "downloading" && (
        <div className="flex flex-col gap-1">
          <div className="h-1 w-full overflow-hidden rounded-full bg-white/10">
            <div
              className="h-full rounded-full bg-(--floe-accent) transition-all duration-300"
              style={{ width: `${updateInfo?.downloadProgress ?? 0}%` }}
            />
          </div>
          <span className="text-[11px] text-white/40">
            Downloading... {Math.round(updateInfo?.downloadProgress ?? 0)}%
          </span>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center gap-3">
        {status === "idle" || status === "no_update" || status === "error" ? (
          <button
            type="button"
            onClick={handleCheckForUpdate}
            disabled={updateCheckInProgress}
            className={cn(
              "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-all duration-200",
              updateCheckInProgress
                ? "cursor-not-allowed bg-white/5 text-white/30"
                : "bg-white/10 text-white/70 hover:bg-white/15 hover:text-white/90",
            )}
          >
            <RefreshCw
              width={12}
              height={12}
              strokeWidth={1.5}
              className={cn(updateCheckInProgress && "animate-spin")}
            />
            {updateCheckInProgress
              ? "Checking\u2026"
              : status === "error"
                ? "Retry check"
                : "Check for updates"}
          </button>
        ) : null}

        {status === "available" && (
          <button
            type="button"
            onClick={handleDownloadUpdate}
            className="flex items-center gap-1.5 rounded-md bg-(--floe-accent) px-3 py-1.5 text-xs font-medium text-white transition-all hover:brightness-110"
          >
            <Download width={12} height={12} strokeWidth={1.5} />
            Download update
          </button>
        )}

        {status === "downloaded" && (
          <button
            type="button"
            onClick={handleInstallUpdate}
            className="flex items-center gap-1.5 rounded-md bg-(--floe-accent) px-3 py-1.5 text-xs font-medium text-white transition-all hover:brightness-110"
          >
            <RotateCw width={12} height={12} strokeWidth={1.5} />
            Restart to update
          </button>
        )}

        {status === "available" || status === "downloading" ? (
          <button
            type="button"
            onClick={handleDismiss}
            className="rounded-md px-2 py-1.5 text-xs text-white/40 transition-colors hover:text-white/70"
          >
            Dismiss
          </button>
        ) : null}
      </div>
    </div>
  );
}
