export interface HotkeyCaptureEvent {
  altKey: boolean;
  code: string;
  ctrlKey: boolean;
  key: string;
  metaKey: boolean;
  repeat: boolean;
  shiftKey: boolean;
}

export interface CapturedHotkey {
  accelerator: string;
  label: string;
}

const modifierCodes = new Set([
  "AltLeft",
  "AltRight",
  "ControlLeft",
  "ControlRight",
  "MetaLeft",
  "MetaRight",
  "ShiftLeft",
  "ShiftRight",
]);

export function captureHotkey(event: HotkeyCaptureEvent): CapturedHotkey {
  if (event.repeat) {
    throw new Error("Hold one shortcut at a time.");
  }

  if (modifierCodes.has(event.code)) {
    throw new Error("Press a key with at least two modifier keys.");
  }

  const modifiers = modifierParts(event);

  if (modifiers.length < 2) {
    throw new Error("Use at least two modifier keys.");
  }

  if (!event.ctrlKey && !event.altKey && !event.metaKey) {
    throw new Error("Use Control, Alt, Command, or Super in the shortcut.");
  }

  const key = keyPart(event);

  if (key === null) {
    throw new Error("This shortcut is not supported.");
  }

  return {
    accelerator: [
      ...modifiers.map((modifier) => modifier.accelerator),
      key,
    ].join("+"),
    label: [...modifiers.map((modifier) => modifier.label), keyLabel(key)].join(
      "+",
    ),
  };
}

function modifierParts(event: HotkeyCaptureEvent) {
  const modifiers: Array<{ accelerator: string; label: string }> = [];

  if (event.ctrlKey) {
    modifiers.push({ accelerator: "Control", label: "Control" });
  }
  if (event.altKey) {
    modifiers.push({ accelerator: "Alt", label: "Alt" });
  }
  if (event.shiftKey) {
    modifiers.push({ accelerator: "Shift", label: "Shift" });
  }
  if (event.metaKey) {
    modifiers.push({
      accelerator: "Super",
      label: isMacLikePlatform() ? "Command" : "Super",
    });
  }

  return modifiers;
}

function keyPart(event: HotkeyCaptureEvent): string | null {
  if (/^Key[A-Z]$/.test(event.code)) {
    return event.code;
  }

  if (/^Digit[0-9]$/.test(event.code)) {
    return event.code;
  }

  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.code)) {
    return event.code;
  }

  if (
    [
      "Backquote",
      "Backslash",
      "BracketLeft",
      "BracketRight",
      "Comma",
      "Delete",
      "End",
      "Enter",
      "Equal",
      "Escape",
      "Home",
      "Insert",
      "Minus",
      "PageDown",
      "PageUp",
      "Period",
      "Quote",
      "Semicolon",
      "Slash",
      "Space",
      "Tab",
    ].includes(event.code)
  ) {
    return event.code;
  }

  if (["ArrowDown", "ArrowLeft", "ArrowRight", "ArrowUp"].includes(event.key)) {
    return event.key;
  }

  return null;
}

function keyLabel(key: string): string {
  return key.replace(/^Key/, "").replace(/^Digit/, "");
}

function isMacLikePlatform(): boolean {
  return /Mac|iPhone|iPad/.test(window.navigator.platform);
}
