import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauri";
import type { RecordingLevelPayload } from "../lib/tauri";
import {
  appendWaveformSample,
  clamp01,
  createSilentWaveformBuffer,
  SILENT_SAMPLE_LEVEL,
  smoothWaveformInput,
} from "../lib/waveform";

const SAMPLE_INTERVAL_MS: number = 40;

export function useRollingWaveform(active: boolean): number[] {
  const [samples, setSamples] = useState<number[]>(() =>
    createSilentWaveformBuffer(),
  );
  const targetLevelRef = useRef(SILENT_SAMPLE_LEVEL);
  const smoothedLevelRef = useRef(SILENT_SAMPLE_LEVEL);
  const rafRef = useRef<number | null>(null);
  const lastSampleAtRef = useRef(0);

  useEffect(() => {
    resetWaveformState();

    if (!active || !isTauriRuntime()) {
      return undefined;
    }

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<RecordingLevelPayload>("recording-level", (event) => {
      targetLevelRef.current = clamp01(event.payload.level);
    })
      .then((nextUnlisten) => {
        if (cancelled) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch(() => {
        // Level events are visual-only; failure leaves the bubble at silence.
      });

    rafRef.current = requestAnimationFrame(tick);

    return () => {
      cancelled = true;
      unlisten?.();
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };

    function tick(timestamp: number): void {
      if (cancelled) {
        return;
      }

      if (
        lastSampleAtRef.current === 0 ||
        timestamp - lastSampleAtRef.current >= SAMPLE_INTERVAL_MS
      ) {
        lastSampleAtRef.current = timestamp;
        smoothedLevelRef.current = smoothWaveformInput(
          smoothedLevelRef.current,
          targetLevelRef.current,
        );
        setSamples((current) =>
          appendWaveformSample(current, smoothedLevelRef.current),
        );
      }

      rafRef.current = requestAnimationFrame(tick);
    }

    function resetWaveformState(): void {
      targetLevelRef.current = SILENT_SAMPLE_LEVEL;
      smoothedLevelRef.current = SILENT_SAMPLE_LEVEL;
      lastSampleAtRef.current = 0;
      setSamples(createSilentWaveformBuffer());
    }
  }, [active]);

  return samples;
}
