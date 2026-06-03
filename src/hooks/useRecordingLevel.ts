import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauri";
import type { RecordingLevelPayload } from "../lib/tauri";

const FRONTEND_ATTACK: number = 0.6;
const FRONTEND_RELEASE: number = 0.18;
const RENDER_INTERVAL_MS: number = 50;

export function useRecordingLevel(active: boolean): number {
  const [level, setLevel] = useState(0);
  const levelRef = useRef(0);
  const smoothedRef = useRef(0);
  const rafRef = useRef<number | null>(null);
  const lastRenderRef = useRef(0);

  useEffect(() => {
    if (!active || !isTauriRuntime()) {
      levelRef.current = 0;
      smoothedRef.current = 0;
      setLevel(0);
      return undefined;
    }

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<RecordingLevelPayload>("recording-level", (event) => {
      const next = clamp01(event.payload.level);
      const smoothed = smoothOnePole(smoothedRef.current, next);
      smoothedRef.current = smoothed;
      levelRef.current = smoothed;
      scheduleRender();
    })
      .then((nextUnlisten) => {
        if (cancelled) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch(() => {
        // Event subscription is best-effort; if it fails the bubble will
        // simply stay at zero rather than block the UI.
      });

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };

    function scheduleRender(): void {
      if (rafRef.current !== null) {
        return;
      }
      rafRef.current = requestAnimationFrame((timestamp) => {
        rafRef.current = null;
        if (timestamp - lastRenderRef.current < RENDER_INTERVAL_MS) {
          scheduleRender();
          return;
        }
        lastRenderRef.current = timestamp;
        setLevel(levelRef.current);
      });
    }
  }, [active]);

  return level;
}

export function smoothOnePole(previous: number, next: number): number {
  const coefficient = next > previous ? FRONTEND_ATTACK : FRONTEND_RELEASE;
  const smoothed = previous + (next - previous) * coefficient;
  return clamp01(smoothed);
}

export function clamp01(value: number): number {
  if (Number.isNaN(value) || !Number.isFinite(value)) {
    return 0;
  }
  if (value < 0) {
    return 0;
  }
  if (value > 1) {
    return 1;
  }
  return value;
}
