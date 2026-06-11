import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { OnboardingView } from "./OnboardingView";
import type { HotkeyStatus } from "../types/app";

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
  vi.restoreAllMocks();
});

const registeredStatus: HotkeyStatus = {
  accelerator: "Control+Space",
  label: "Ctrl + Space",
  isDefault: true,
  isRegistered: true,
  error: null,
};

describe("OnboardingView", () => {
  it("renders the API key step when step is setup_api_key", () => {
    const { container } = renderOnboarding({ step: "setup_api_key" });

    expect(container.textContent).toContain("Floe");
    expect(container.textContent).toContain("API key");
    expect(container.querySelector(".setup-step__input")).not.toBeNull();
  });

  it("renders the Hotkey step when step is setup_hotkey", () => {
    const { container } = renderOnboarding({ step: "setup_hotkey" });

    expect(container.textContent).toContain("Hotkey");
    expect(container.textContent).toContain("Ctrl + Space");
  });

  it("does not render explanatory or marketing copy", () => {
    const apiKey = renderOnboarding({ step: "setup_api_key" });
    const hotkey = renderOnboarding({ step: "setup_hotkey" });

    for (const view of [apiKey, hotkey]) {
      const text = view.container.textContent ?? "";
      expect(text).not.toContain("hold to dictate");
      expect(text).not.toContain("Cerebras");
      expect(text).not.toContain("Behavior");
    }
  });
});

interface RenderOptions {
  step: "setup_api_key" | "setup_hotkey";
  onSaveApiKey?: (value: string) => Promise<void> | void;
  onChangeHotkey?: (accelerator: string) => Promise<void> | void;
  onComplete?: () => void;
}

function renderOnboarding(options: RenderOptions) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  act(() => {
    root.render(
      <OnboardingView
        step={options.step}
        hotkeyStatus={registeredStatus}
        onSaveApiKey={options.onSaveApiKey ?? vi.fn()}
        onChangeHotkey={options.onChangeHotkey ?? vi.fn()}
        onComplete={options.onComplete ?? vi.fn()}
      />,
    );
  });

  return { container };
}
