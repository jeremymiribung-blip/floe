import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauri";
import { EVENT_BUBBLE_STATE } from "../lib/contract";
import type { BubbleStatePayload } from "../lib/contract";

export function useBubbleRecordingActive(): boolean {
  const [active, setActive] = useState(() => !isTauriRuntime());

  useEffect(() => {
    if (!isTauriRuntime()) {
      return undefined;
    }

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<BubbleStatePayload>(EVENT_BUBBLE_STATE, (event) => {
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
