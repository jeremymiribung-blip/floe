import { useRecordingEvents } from "../../hooks/useRecordingEvents";
import { RecordingDot } from "./RecordingDot";

/**
 * Root component for the recording overlay window.
 *
 * Sets up the event listener for bubble-state changes
 * and renders the visual recording dot.
 */
export function Overlay() {
  useRecordingEvents();
  return <RecordingDot />;
}
