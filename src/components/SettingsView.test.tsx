import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsView } from "./SettingsView";
import type {
  GroqApiKeyStatus,
  HotkeyStatus,
  StartAtLoginStatus,
} from "../types/app";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

const groqStatus: GroqApiKeyStatus = {
  configured: false,
  maskedPreview: null,
};
const hotkeyStatus: HotkeyStatus = {
  configured: {
    accelerator: "Control+Space",
    label: "Ctrl + Space",
  },
  registered: {
    accelerator: "Control+Space",
    label: "Ctrl + Space",
  },
  isRegistered: true,
  registrationError: null,
};

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

describe("SettingsView", () => {
  it("renders start at login without a Behavior section", () => {
    const { container } = renderSettingsView();

    expect(container.textContent).toContain("Start at login");
    expect(container.textContent).not.toContain("Behavior");
  });

  it("uses a single API Key heading", () => {
    const { container } = renderSettingsView();

    expect(container.textContent).toContain("API Key");
    expect(container.textContent).not.toContain("API Keys");
  });

  it("does not render any cleanup mode selector", () => {
    const { container } = renderSettingsView();

    expect(container.textContent).not.toContain("Cleanup");
    expect(container.textContent).not.toContain("Behavior");
    expect(container.querySelector("select")).toBeNull();
    expect(container.querySelector('input[name="cleanupMode"]')).toBeNull();
  });

  it("keeps the privacy note about sending text to Groq", () => {
    const { container } = renderSettingsView();

    expect(container.textContent).toContain("Audio → Groq");
    expect(container.textContent).toContain("Text → Groq");
  });

  it("shows the configured hotkey label in the new Ctrl + Space format", () => {
    const { container } = renderSettingsView();

    expect(container.textContent).toContain("Ctrl + Space");
  });

  it("toggles start at login and updates the visible state", async () => {
    const onChange = vi.fn(async () => undefined);
    const { container } = renderSettingsView({ onChange });
    const toggle = startAtLoginToggle(container);

    expect(toggle.textContent).toBe("Off");

    await act(async () => {
      toggle.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onChange).toHaveBeenCalledWith(true);
    expect(startAtLoginToggle(container).textContent).toBe("On");
  });

  it("shows a short friendly error when start at login fails", async () => {
    const { container } = renderSettingsView({
      onChange: async () => {
        throw {
          code: "enableFailed",
          message: "Could not enable start at login",
        };
      },
    });

    await act(async () => {
      startAtLoginToggle(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(container.textContent).toContain("Could not enable start at login");
  });
});

function renderSettingsView(
  options: { onChange?: (enabled: boolean) => Promise<void> } = {},
) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  function Harness() {
    const [startAtLoginStatus, setStartAtLoginStatus] =
      useState<StartAtLoginStatus>({
        enabled: false,
        available: true,
      });

    return (
      <SettingsView
        groqStatus={groqStatus}
        hotkeyStatus={hotkeyStatus}
        startAtLoginStatus={startAtLoginStatus}
        onClose={() => undefined}
        onSaveGroq={() => undefined}
        onClearGroq={() => undefined}
        onChangeHotkey={() => undefined}
        onResetHotkey={() => undefined}
        onSetStartAtLogin={async (enabled) => {
          await options.onChange?.(enabled);
          setStartAtLoginStatus({
            enabled,
            available: true,
          });
        }}
      />
    );
  }

  act(() => {
    root.render(<Harness />);
  });

  return { container };
}

function startAtLoginToggle(container: HTMLElement): HTMLButtonElement {
  const toggle = container.querySelector(
    ".start-at-login-row__toggle",
  ) as HTMLButtonElement | null;

  if (!toggle) {
    throw new Error("Start at login toggle not found");
  }

  return toggle;
}
