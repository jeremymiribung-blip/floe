import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { AudioBars } from "./AudioBars";
import {
  createSilentWaveformBuffer,
  WAVEFORM_SAMPLE_COUNT,
} from "../lib/waveform";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

let roots: Root[] = [];
let containers: HTMLElement[] = [];

afterEach(() => {
  for (const root of roots) {
    act(() => root.unmount());
  }
  for (const container of containers) {
    container.remove();
  }
  roots = [];
  containers = [];
});

describe("AudioBars", () => {
  it("renders the rolling waveform buffer", () => {
    const { container } = render(
      <AudioBars samples={createSilentWaveformBuffer()} />,
    );

    const bars = container.querySelectorAll(".audio-bars__bar");
    expect(bars.length).toBe(WAVEFORM_SAMPLE_COUNT);
  });

  it("renders no text nodes", () => {
    const { container } = render(
      <AudioBars samples={createSilentWaveformBuffer()} />,
    );

    expect(container.textContent).toBe("");
  });

  it("uses the audio-bars container class", () => {
    const { container } = render(
      <AudioBars samples={createSilentWaveformBuffer()} />,
    );

    expect(container.querySelector(".audio-bars")).not.toBeNull();
  });

  it("renders louder samples as taller bars", () => {
    const { container } = render(<AudioBars samples={[0, 1]} />);

    const bars = Array.from(
      container.querySelectorAll<HTMLElement>(".audio-bars__bar"),
    );
    const quietHeight = parseHeightPercent(bars[0].style.height);
    const loudHeight = parseHeightPercent(bars[1].style.height);

    expect(loudHeight).toBeGreaterThan(quietHeight);
  });

  it("clamps silent level to a minimum bar height", () => {
    const { container } = render(<AudioBars samples={[0, 0, 0]} />);

    const bars = Array.from(
      container.querySelectorAll<HTMLElement>(".audio-bars__bar"),
    );
    for (const bar of bars) {
      const height = parseHeightPercent(bar.style.height);
      expect(height).toBeGreaterThanOrEqual(18);
    }
  });

  it("clamps to maximum at high levels", () => {
    const { container } = render(<AudioBars samples={[1.5, 2]} />);

    const bars = Array.from(
      container.querySelectorAll<HTMLElement>(".audio-bars__bar"),
    );
    for (const bar of bars) {
      const height = parseHeightPercent(bar.style.height);
      expect(height).toBeLessThanOrEqual(100);
    }
  });

  it("renders zero height for negative level", () => {
    const { container } = render(<AudioBars samples={[-1, -0.5]} />);

    const bars = Array.from(
      container.querySelectorAll<HTMLElement>(".audio-bars__bar"),
    );
    for (const bar of bars) {
      const height = parseHeightPercent(bar.style.height);
      expect(height).toBeGreaterThanOrEqual(0);
    }
  });
});

function render(element: React.ReactElement): { container: HTMLElement } {
  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);
  act(() => {
    root.render(element);
  });
  return { container };
}

function parseHeightPercent(style: string): number {
  const match = /([\d.]+)%/.exec(style);
  if (!match) {
    return 0;
  }
  return Number(match[1]);
}
