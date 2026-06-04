import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GroqSetupStep } from "./GroqSetupStep";

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

describe("GroqSetupStep", () => {
  it("disables Continue until a non-empty value is entered", () => {
    const { container } = renderStep();

    const button = continueButton(container);
    expect(button.hasAttribute("disabled")).toBe(true);

    act(() => {
      setInputValue(input(container), "gsk_test");
    });

    expect(button.hasAttribute("disabled")).toBe(false);
  });

  it("calls onContinue with the entered value and disables the button while submitting", async () => {
    let resolve: (() => void) | null = null;
    const onContinue = vi.fn(
      () =>
        new Promise<void>((r) => {
          resolve = r;
        }),
    );
    const { container } = renderStep({ onContinue });

    act(() => {
      setInputValue(input(container), "gsk_12345678abcd");
    });

    act(() => {
      form(container).dispatchEvent(
        new Event("submit", { bubbles: true, cancelable: true }),
      );
    });

    expect(onContinue).toHaveBeenCalledWith("gsk_12345678abcd");
    expect(continueButton(container).hasAttribute("disabled")).toBe(true);

    await act(async () => {
      resolve?.();
    });
  });

  it("shows 'Could not save key' when onContinue rejects", async () => {
    const onContinue = vi.fn(async () => {
      throw new Error("storage unavailable");
    });
    const { container } = renderStep({ onContinue });

    act(() => {
      setInputValue(input(container), "gsk_12345678abcd");
    });

    await act(async () => {
      form(container).dispatchEvent(
        new Event("submit", { bubbles: true, cancelable: true }),
      );
    });

    expect(container.textContent).toContain("Could not save key");
    expect(continueButton(container).hasAttribute("disabled")).toBe(false);
  });
});

interface RenderOptions {
  onContinue?: (value: string) => Promise<void> | void;
  busy?: boolean;
}

function renderStep(options: RenderOptions = {}) {
  const onContinue = options.onContinue ?? vi.fn();
  const busy = options.busy ?? false;

  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  act(() => {
    root.render(
      <GroqSetupStep
        busy={busy}
        onContinue={async (next) => {
          await onContinue(next);
        }}
      />,
    );
  });

  return { container };
}

function setInputValue(input: HTMLInputElement, value: string) {
  const nativeSetter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  nativeSetter?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function input(container: HTMLElement): HTMLInputElement {
  return container.querySelector(".setup-step__input") as HTMLInputElement;
}

function form(container: HTMLElement): HTMLFormElement {
  return container.querySelector(".setup-step") as HTMLFormElement;
}

function continueButton(container: HTMLElement): HTMLButtonElement {
  return container.querySelector(
    ".setup-step__button--primary",
  ) as HTMLButtonElement;
}
