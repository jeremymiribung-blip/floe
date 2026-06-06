import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HotkeySetupStep } from "./HotkeySetupStep";
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

const unregisteredStatus: HotkeyStatus = {
  accelerator: "Control+Space",
  label: "Ctrl + Space",
  isDefault: true,
  isRegistered: false,
  error: "Hotkey unavailable",
};

describe("HotkeySetupStep", () => {
  it("shows Loading only while hotkey status is unknown", () => {
    const { container } = renderStep({ hotkeyStatus: null });

    expect(container.textContent).toContain("Loading");
    expect(container.textContent).not.toContain("Hotkey unavailable");
    expect(continueButton(container).hasAttribute("disabled")).toBe(true);
  });

  it("renders the current hotkey label and Change / Continue buttons", () => {
    const { container } = renderStep();

    expect(container.textContent).toContain("Ctrl + Space");
    expect(changeButton(container).textContent).toBe("Change");
    expect(continueButton(container).textContent).toBe("Continue");
  });

  it("renders the macOS default label when the default hotkey is registered", () => {
    const { container } = renderStep({
      hotkeyStatus: {
        accelerator: "Alt+Space",
        label: "Option + Space",
        isDefault: true,
        isRegistered: true,
        error: null,
      },
    });

    expect(container.textContent).toContain("Option + Space");
    expect(continueButton(container).hasAttribute("disabled")).toBe(false);
  });

  it("enables Continue for a registered default hotkey", () => {
    const { container } = renderStep({ hotkeyStatus: registeredStatus });

    expect(continueButton(container).hasAttribute("disabled")).toBe(false);
  });

  it("disables Continue while a hotkey is not registered", () => {
    const { container } = renderStep({ hotkeyStatus: unregisteredStatus });

    expect(container.textContent).toContain("Hotkey unavailable");
    expect(container.textContent).not.toContain("Loading");
    expect(continueButton(container).hasAttribute("disabled")).toBe(true);
  });

  it("calls onContinue when Continue is pressed with a valid hotkey", () => {
    const onContinue = vi.fn();
    const { container } = renderStep({ onContinue });

    act(() => {
      continueButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(onContinue).toHaveBeenCalledOnce();
  });

  it("enters capture mode and saves a valid shortcut", async () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    const { container } = renderStep({ onChange });

    act(() => {
      changeButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(container.textContent).toContain("Press shortcut");

    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: " ",
          code: "Space",
          ctrlKey: true,
          altKey: false,
          shiftKey: false,
          metaKey: false,
        }),
      );
    });

    expect(onChange).toHaveBeenCalledWith("Control+Space");
    expect(container.textContent).not.toContain("Press shortcut");
  });

  it("cancels capture on Escape and keeps the current hotkey", () => {
    const onChange = vi.fn();
    const { container } = renderStep({ onChange });

    act(() => {
      changeButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(container.textContent).toContain("Press shortcut");

    act(() => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", code: "Escape" }),
      );
    });

    expect(onChange).not.toHaveBeenCalled();
    expect(container.textContent).not.toContain("Press shortcut");
    expect(container.textContent).toContain("Ctrl + Space");
  });

  it("shows 'Hotkey unavailable' when registration fails", async () => {
    const onChange = vi.fn().mockRejectedValue(new Error("busy"));
    const { container } = renderStep({ onChange });

    act(() => {
      changeButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: " ",
          code: "Space",
          ctrlKey: true,
          altKey: false,
          shiftKey: false,
          metaKey: false,
        }),
      );
    });

    expect(container.textContent).toContain("Hotkey unavailable");
    expect(container.textContent).not.toContain("Press shortcut");
  });
});

interface RenderOptions {
  hotkeyStatus?: HotkeyStatus | null;
  onChange?: (accelerator: string) => Promise<void> | void;
  onContinue?: () => void;
  busy?: boolean;
}

function renderStep(options: RenderOptions = {}) {
  const hotkeyStatus: HotkeyStatus | null =
    "hotkeyStatus" in options
      ? (options.hotkeyStatus ?? null)
      : registeredStatus;
  const onChange = options.onChange ?? vi.fn();
  const onContinue = options.onContinue ?? vi.fn();
  const busy = options.busy ?? false;

  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  act(() => {
    root.render(
      <HotkeySetupStep
        hotkeyStatus={hotkeyStatus}
        busy={busy}
        onChange={async (accelerator) => {
          await onChange(accelerator);
        }}
        onContinue={onContinue}
      />,
    );
  });

  return { container };
}

function changeButton(container: HTMLElement): HTMLButtonElement {
  return container.querySelector(
    ".setup-step__button--primary",
  ) as HTMLButtonElement;
}

function continueButton(container: HTMLElement): HTMLButtonElement {
  const buttons = container.querySelectorAll(
    ".setup-step__button",
  ) as NodeListOf<HTMLButtonElement>;
  return buttons[buttons.length - 1];
}
