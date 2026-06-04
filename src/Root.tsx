import { useLayoutEffect } from "react";
import { RecordingBubble } from "./components/RecordingBubble";
import { isOverlayWindow } from "./lib/windowLabel";
import App from "./App";

const BUBBLE_WINDOW_CLASS = "bubble-window";

export function Root() {
  const isBubble = isOverlayWindow();

  useLayoutEffect(() => {
    if (!isBubble) {
      return undefined;
    }
    document.documentElement.classList.add(BUBBLE_WINDOW_CLASS);
    return () => {
      document.documentElement.classList.remove(BUBBLE_WINDOW_CLASS);
    };
  }, [isBubble]);

  if (isBubble) {
    return <RecordingBubble />;
  }
  return <App />;
}
