import type { RecordingEndReason, RecordingInfo } from "../types/app";

const endReasonLabels: Record<RecordingEndReason, string> = {
  manual: "Stopped manually",
  maxDuration: "Stopped at max duration",
  deviceDisconnected: "Device disconnected",
  shutdown: "Stopped during shutdown",
  watchdogTimeout: "Stopped after timeout",
};

export function formatDurationMs(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;

  if (minutes === 0) {
    return `${seconds}s`;
  }

  return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
}

export function formatRecordingInfo(info: RecordingInfo): string {
  return [
    formatDurationMs(info.durationMs),
    `${info.sampleRate} Hz input`,
    `${info.wavSampleRate} Hz ${info.wavFormat.toUpperCase()}`,
    `${info.inputChannels}->${info.outputChannels} channel`,
    `${info.sampleCount} samples`,
    `${info.wavByteCount} WAV bytes`,
    `${info.wavBitsPerSample}-bit PCM`,
    endReasonLabels[info.endedReason],
  ].join(" | ");
}
