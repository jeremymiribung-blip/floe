import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { OverviewView } from "./OverviewView";

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

describe("OverviewView", () => {
  it("renders the wordmark, status, hotkey, and a Settings link", () => {
    const onOpenSettings = vi.fn();
    const { container } = renderOverview({
      status: "Ready",
      hotkeyLabel: "Ctrl + Space",
      onOpenSettings,
    });

    expect(container.textContent).toContain("Floe");
    expect(container.textContent).toContain("Ready");
    expect(container.textContent).toContain("Ctrl + Space");

    const settings = container.querySelector(
      ".overview-view__settings",
    ) as HTMLButtonElement | null;
    expect(settings?.textContent).toBe("Settings");
  });

  it("calls onOpenSettings when Settings is clicked", () => {
    const onOpenSettings = vi.fn();
    const { container } = renderOverview({
      status: "Ready",
      hotkeyLabel: "Ctrl + Space",
      onOpenSettings,
    });

    act(() => {
      (
        container.querySelector(".overview-view__settings") as HTMLButtonElement
      ).dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onOpenSettings).toHaveBeenCalledOnce();
  });

  it("does not include cleanup mode, Behavior, or Cerebras copy", () => {
    const { container } = renderOverview({
      status: "Ready",
      hotkeyLabel: "Ctrl + Space",
      onOpenSettings: vi.fn(),
    });

    const text = container.textContent ?? "";
    expect(text).not.toContain("Cleanup");
    expect(text).not.toContain("Behavior");
    expect(text).not.toContain("Cerebras");
    expect(text).not.toContain("Raw");
    expect(text).not.toContain("Fast");
    expect(text).not.toContain("Clean");
    expect(text).not.toContain("hold to dictate");
  });

  it("shows the dynamic status line and not a separate error paragraph", () => {
    const { container } = renderOverview({
      status: "Hotkey unavailable",
      hotkeyLabel: "Ctrl + Space",
      onOpenSettings: vi.fn(),
    });

    expect(container.textContent).toContain("Hotkey unavailable");
    expect(container.querySelector(".overview-view__error")).toBeNull();
  });
});

interface RenderOptions {
  status: string;
  hotkeyLabel: string;
  onOpenSettings: () => void;
}

function renderOverview(options: RenderOptions) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  act(() => {
    root.render(
      <OverviewView
        status={options.status}
        hotkeyLabel={options.hotkeyLabel}
        onOpenSettings={options.onOpenSettings}
      />,
    );
  });

  return { container };
}
