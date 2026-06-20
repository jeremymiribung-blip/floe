import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { RecordingDot } from "./RecordingDot";
import { useRecordingStore } from "../../stores/recording";
import { createSilentWaveformBuffer } from "../../lib/waveform";

vi.mock("../../hooks/useRollingWaveform", () => ({
  useRollingWaveform: () => createSilentWaveformBuffer(),
}));

beforeEach(() => {
  useRecordingStore.setState({ overlayState: "hidden" });
});

afterEach(() => {
  cleanup();
});

describe("RecordingDot", () => {
  it("renders nothing when state is hidden", () => {
    useRecordingStore.setState({ overlayState: "hidden" });
    const { container } = render(<RecordingDot />);
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing when state is processing", () => {
    useRecordingStore.setState({ overlayState: "processing" });
    const { container } = render(<RecordingDot />);
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing when state is success", () => {
    useRecordingStore.setState({ overlayState: "success" });
    const { container } = render(<RecordingDot />);
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing when state is error", () => {
    useRecordingStore.setState({ overlayState: "error" });
    const { container } = render(<RecordingDot />);
    expect(container.innerHTML).toBe("");
  });

  it("renders with role='status' when active", () => {
    useRecordingStore.setState({ overlayState: "active" });
    const { getByRole } = render(<RecordingDot />);
    expect(getByRole("status")).toBeDefined();
  });

  it("has correct aria-label when active", () => {
    useRecordingStore.setState({ overlayState: "active" });
    const { getByRole } = render(<RecordingDot />);
    expect(getByRole("status").getAttribute("aria-label")).toBe("Recording");
  });

  it("renders audio bars when active", () => {
    useRecordingStore.setState({ overlayState: "active" });
    const { container } = render(<RecordingDot />);
    expect(container.querySelector(".audio-bars")).not.toBeNull();
  });

  it("renders no text nodes when active", () => {
    useRecordingStore.setState({ overlayState: "active" });
    const { container } = render(<RecordingDot />);
    expect(container.textContent).toBe("");
  });
});
