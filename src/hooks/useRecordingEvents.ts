import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { EVENT_BUBBLE_STATE } from "../lib/contract";
import type { BubbleStatePayload } from "../types/app";
import { useRecordingStore } from "../stores/recording";

/**
 * Listens for bubble-state events from the backend and
 * updates the Zustand recording store.
 *
 * Only active in the overlay window.
 */
export function useRecordingEvents() {
  const setOverlayState = useRecordingStore((s) => s.setOverlayState);

  useEffect(() => {
    const unlisten = listen<BubbleStatePayload>(EVENT_BUBBLE_STATE, (event) => {
      setOverlayState(event.payload.bubbleState);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [setOverlayState]);
}
