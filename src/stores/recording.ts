import { create } from "zustand";

export type OverlayState =
  | "hidden"
  | "active"
  | "processing"
  | "success"
  | "error";

interface RecordingStore {
  overlayState: OverlayState;
  setOverlayState: (state: OverlayState) => void;
  reset: () => void;
}

export const useRecordingStore = create<RecordingStore>((set) => ({
  overlayState: "hidden",
  setOverlayState: (overlayState) => set({ overlayState }),
  reset: () => set({ overlayState: "hidden" }),
}));
