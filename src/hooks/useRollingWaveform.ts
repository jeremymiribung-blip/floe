import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauri";
import type { RecordingLevelPayload } from "../lib/contract";
import {
  appendWaveformSample,
  clamp01,
  createSilentWaveformBuffer,
  SILENT_SAMPLE_LEVEL,
  WAVEFORM_BUCKET_MS,
} from "../lib/waveform";

export function useRollingWaveform(active: boolean): number[] {
  const [samples, setSamples] = useState<number[]>(() =>
    createSilentWaveformBuffer(),
  );
  const bucketMaxRef = useRef(SILENT_SAMPLE_LEVEL);
  const bucketStartRef = useRef(0);
  const bucketInitializedRef = useRef(false);
  const rafRef = useRef<number | null>(null);

  useEffect(() => {
    resetWaveformState();

    if (!active || !isTauriRuntime()) {
      return undefined;
    }

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<RecordingLevelPayload>("recording-level", (event) => {
      const level = clamp01(event.payload.level);
      if (level > bucketMaxRef.current) {
        bucketMaxRef.current = level;
      }
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

      if (!bucketInitializedRef.current) {
        bucketStartRef.current = timestamp;
        bucketInitializedRef.current = true;
      }

      while (timestamp - bucketStartRef.current >= WAVEFORM_BUCKET_MS) {
        const finalized = bucketMaxRef.current;
        setSamples((current) => appendWaveformSample(current, finalized));
        bucketMaxRef.current = SILENT_SAMPLE_LEVEL;
        bucketStartRef.current += WAVEFORM_BUCKET_MS;
      }

      rafRef.current = requestAnimationFrame(tick);
    }

    function resetWaveformState(): void {
      bucketMaxRef.current = SILENT_SAMPLE_LEVEL;
      bucketStartRef.current = 0;
      bucketInitializedRef.current = false;
      setSamples(createSilentWaveformBuffer());
    }
  }, [active]);

  return samples;
}
