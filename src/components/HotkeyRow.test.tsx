import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HotkeyRow } from "./HotkeyRow";
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

describe("HotkeyRow", () => {
  it("shows Loading only while hotkey status is unknown", () => {
    const { container } = renderHotkeyRow({ hotkeyStatus: null });

    expect(container.textContent).toContain("Loading");
    expect(container.textContent).not.toContain("Hotkey unavailable");
  });

  it("renders the current hotkey label and Change / Reset buttons", () => {
    const { container } = renderHotkeyRow({
      hotkeyStatus: makeStatus("Ctrl + Space", "Control+Space"),
    });

    expect(container.textContent).toContain("Ctrl + Space");
    expect(
      container.querySelector(".hotkey-row__button--primary")?.textContent,
    ).toBe("Change");
    expect(
      container.querySelectorAll(".hotkey-row__button")[1]?.textContent,
    ).toBe("Reset");
  });

  it("shows unavailable status when the configured hotkey is not registered", () => {
    const { container } = renderHotkeyRow({
      hotkeyStatus: {
        accelerator: "Control+Space",
        label: "Ctrl + Space",
        isDefault: true,
        isRegistered: false,
        error: "Hotkey unavailable",
      },
    });

    expect(container.textContent).toContain("Hotkey unavailable");
    expect(container.textContent).not.toContain("Loading");
  });

  it("enters capture mode and saves a valid shortcut", async () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    const { container } = renderHotkeyRow({ onChange });

    act(() => {
      changeButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(container.textContent).toContain("Press shortcut");
    expect(container.textContent).toContain("Cancel");

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

  it("cancels capture when Escape is pressed without calling onChange", () => {
    const onChange = vi.fn();
    const { container } = renderHotkeyRow({ onChange });

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
    expect(
      container.querySelector(".hotkey-row__button--primary")?.textContent,
    ).toBe("Change");
  });

  it("shows a validation message for plain Space and does not call onChange", () => {
    const onChange = vi.fn();
    const { container } = renderHotkeyRow({ onChange });

    act(() => {
      changeButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    act(() => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: " ",
          code: "Space",
          ctrlKey: false,
          altKey: false,
          shiftKey: false,
          metaKey: false,
        }),
      );
    });

    expect(onChange).not.toHaveBeenCalled();
    expect(container.textContent).toContain(
      "Press a key with at least one modifier.",
    );
    expect(
      container.querySelector(".hotkey-row__button--primary")?.textContent,
    ).toBe("Change");
  });

  it("calls onReset when the Reset button is clicked", () => {
    const onReset = vi.fn();
    const { container } = renderHotkeyRow({ onReset });

    act(() => {
      resetButton(container).dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(onReset).toHaveBeenCalledOnce();
  });

  it("surfaces onChange errors via the capture message", async () => {
    const onChange = vi.fn().mockRejectedValue(new Error("Hotkey unavailable"));
    const { container } = renderHotkeyRow({ onChange });

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
    expect(
      container.querySelector(".hotkey-row__button--primary")?.textContent,
    ).toBe("Change");
  });
});

function makeStatus(label: string, accelerator: string): HotkeyStatus {
  return {
    accelerator,
    label,
    isDefault: accelerator === "Control+Space",
    isRegistered: true,
    error: null,
  };
}

interface RenderOptions {
  hotkeyStatus?: HotkeyStatus | null;
  onChange?: (accelerator: string) => Promise<void> | void;
  onReset?: () => Promise<void> | void;
}

function renderHotkeyRow(options: RenderOptions = {}) {
  const hotkeyStatus: HotkeyStatus | null =
    "hotkeyStatus" in options
      ? (options.hotkeyStatus ?? null)
      : makeStatus("Ctrl + Space", "Control+Space");
  const onChange = options.onChange ?? vi.fn();
  const onReset = options.onReset ?? vi.fn();

  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  function Harness() {
    const [status] = useState<HotkeyStatus | null>(hotkeyStatus);
    return (
      <HotkeyRow
        hotkeyStatus={status}
        onChange={async (accelerator) => {
          await onChange(accelerator);
        }}
        onReset={async () => {
          await onReset();
        }}
      />
    );
  }

  act(() => {
    root.render(<Harness />);
  });

  return { container };
}

function changeButton(container: HTMLElement): HTMLButtonElement {
  const button = container.querySelector(
    ".hotkey-row__button--primary",
  ) as HTMLButtonElement | null;
  if (!button) {
    throw new Error("Change button not found");
  }
  return button;
}

function resetButton(container: HTMLElement): HTMLButtonElement {
  const buttons = container.querySelectorAll(
    ".hotkey-row__button",
  ) as NodeListOf<HTMLButtonElement>;
  if (buttons.length < 2) {
    throw new Error("Reset button not found");
  }
  return buttons[1];
}
