import { RecordingBubble } from "./components/RecordingBubble";
import { isOverlayWindow } from "./lib/windowLabel";
import App from "./App";

export function Root() {
  if (isOverlayWindow()) {
    return <RecordingBubble />;
  }
  return <App />;
}
