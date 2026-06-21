import { create } from "zustand";
import type { AppState, UpdateInfo, AudioDevice } from "../types/app";

export type FloeStatus = "idle" | "recording" | "processing";

function appStateToFloeStatus(state: AppState): FloeStatus {
  switch (state) {
    case "recording":
    case "starting":
      return "recording";
    case "stopping":
    case "transcribing":
    case "cleaning":
    case "preview":
    case "pasting":
    case "pasted":
    case "copied":
      return "processing";
    case "idle":
    case "ready":
    case "error":
      return "idle";
  }
}

export interface FloeState {
  /* ── State ─────────────────────────────────────────────── */
  status: FloeStatus;
  recordingStartedAt: number | null;
  recordingDurationMs: number;
  apiKey: string | null;
  apiKeyConfigured: boolean;
  apiKeyMaskedPreview: string | null;
  hotkey: string | null;
  isSettingsOpen: boolean;
  isHotkeyCaptureActive: boolean;
  launchOnStartup: boolean;
  audioDevices: AudioDevice[];
  selectedAudioDeviceId: string | null;
  skipCleanup: boolean;

  /* ── Update state ─────────────────────────────────────── */
  updateInfo: UpdateInfo | null;
  updateCheckInProgress: boolean;

  /* ── Actions ───────────────────────────────────────────── */
  syncFromPipeline: (appState: AppState) => void;
  setRecordingStartedAt: (startedAt: number | null) => void;
  tickRecording: (now: number) => void;
  startRecording: () => void;
  stopRecordingAndProcess: () => void;
  resetToIdle: () => void;
  setApiKey: (apiKey: string) => void;
  setApiKeyStatus: (configured: boolean, maskedPreview: string | null) => void;
  setHotkey: (hotkey: string) => void;
  openSettings: () => void;
  closeSettings: () => void;
  toggleSettings: () => void;
  startHotkeyCapture: () => void;
  stopHotkeyCapture: () => void;
  setLaunchOnStartup: (value: boolean) => void;
  setAudioDevices: (devices: AudioDevice[]) => void;
  setSelectedAudioDeviceId: (deviceId: string | null) => void;
  setSkipCleanup: (skipCleanup: boolean) => void;
  setUpdateInfo: (info: UpdateInfo | null) => void;
  setUpdateCheckInProgress: (inProgress: boolean) => void;

  /* ── Derived selectors ─────────────────────────────────── */
  isIdle: () => boolean;
  isRecording: () => boolean;
  isProcessing: () => boolean;
}

const useFloeStore = create<FloeState>()((set, get) => ({
  /* ── Initial state ─────────────────────────────────────── */
  status: "idle",
  recordingStartedAt: null,
  recordingDurationMs: 0,
  apiKey: null,
  apiKeyConfigured: false,
  apiKeyMaskedPreview: null,
  hotkey: null,
  isSettingsOpen: false,
  launchOnStartup: false,
  isHotkeyCaptureActive: false,
  audioDevices: [],
  selectedAudioDeviceId: null,
  skipCleanup: false,
  updateInfo: null,
  updateCheckInProgress: false,

  /* ── Pipeline-synced actions ──────────────────────────── */
  syncFromPipeline: (appState: AppState) =>
    set(() => {
      const status = appStateToFloeStatus(appState);
      if (status === "recording") {
        return { status };
      }
      if (status === "processing") {
        return { status };
      }
      return {
        status,
        recordingStartedAt: null,
        recordingDurationMs: 0,
      };
    }),

  setRecordingStartedAt: (recordingStartedAt: number | null) =>
    set({ recordingStartedAt, recordingDurationMs: 0 }),

  tickRecording: (now: number) =>
    set((state) => {
      if (state.status !== "recording" || state.recordingStartedAt === null)
        return {};
      return { recordingDurationMs: now - state.recordingStartedAt };
    }),

  /* ── Recording actions ─────────────────────────────────── */
  startRecording: () =>
    set({
      status: "recording",
      recordingStartedAt: Date.now(),
      recordingDurationMs: 0,
    }),

  stopRecordingAndProcess: () =>
    set({
      status: "processing",
    }),

  /* ── Reset action ──────────────────────────────────────── */
  resetToIdle: () =>
    set({
      status: "idle",
      recordingStartedAt: null,
      recordingDurationMs: 0,
    }),

  /* ── Configuration actions ─────────────────────────────── */
  setApiKey: (apiKey: string) => set({ apiKey }),
  setApiKeyStatus: (configured, maskedPreview) =>
    set({ apiKeyConfigured: configured, apiKeyMaskedPreview: maskedPreview }),
  setHotkey: (hotkey: string) => set({ hotkey }),

  /* ── UI actions ────────────────────────────────────────── */
  openSettings: () => set({ isSettingsOpen: true }),
  closeSettings: () => set({ isSettingsOpen: false }),
  toggleSettings: () =>
    set((state) => ({ isSettingsOpen: !state.isSettingsOpen })),

  startHotkeyCapture: () => set({ isHotkeyCaptureActive: true }),
  stopHotkeyCapture: () => set({ isHotkeyCaptureActive: false }),

  setLaunchOnStartup: (launchOnStartup: boolean) => set({ launchOnStartup }),
  setSkipCleanup: (skipCleanup: boolean) => set({ skipCleanup }),

  /* ── Audio device actions ───────────────────────────────── */
  setAudioDevices: (audioDevices: AudioDevice[]) => set({ audioDevices }),
  setSelectedAudioDeviceId: (selectedAudioDeviceId: string | null) =>
    set({ selectedAudioDeviceId }),

  /* ── Update actions ──────────────────────────────────── */
  setUpdateInfo: (info: UpdateInfo | null) =>
    set({ updateInfo: info, updateCheckInProgress: false }),
  setUpdateCheckInProgress: (inProgress: boolean) =>
    set({ updateCheckInProgress: inProgress }),

  /* ── Derived selectors (computed booleans) ────────────── */
  isIdle: () => get().status === "idle",
  isRecording: () => get().status === "recording",
  isProcessing: () => get().status === "processing",
}));

export default useFloeStore;
