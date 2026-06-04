import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauri";

const BUBBLE_STATE_EVENT: string = "recording-bubble-state";

interface BubbleStatePayload {
  recording: boolean;
}

export function useBubbleRecordingActive(): boolean {
  const [active, setActive] = useState(() => !isTauriRuntime());

  useEffect(() => {
    if (!isTauriRuntime()) {
      return undefined;
    }

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<BubbleStatePayload>(BUBBLE_STATE_EVENT, (event) => {
      setActive(event.payload.recording);
    })
      .then((nextUnlisten) => {
        if (cancelled) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch(() => {
        setActive(false);
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return active;
}
