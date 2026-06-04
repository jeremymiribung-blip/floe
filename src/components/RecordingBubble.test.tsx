import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RecordingBubble } from "./RecordingBubble";
import { createSilentWaveformBuffer } from "../lib/waveform";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("../hooks/useRollingWaveform", () => {
  return {
    useRollingWaveform: () => createSilentWaveformBuffer(),
  };
});

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

describe("RecordingBubble", () => {
  it("renders only the waveform inside the bubble", () => {
    const { container } = render(<RecordingBubble />);

    expect(container.textContent).toBe("");
    expect(container.querySelector(".recording-bubble")).not.toBeNull();
    expect(container.querySelector(".audio-bars")).not.toBeNull();
    expect(container.querySelector("button")).toBeNull();
    expect(container.querySelector("svg")).toBeNull();
    expect(container.querySelector("time")).toBeNull();
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
