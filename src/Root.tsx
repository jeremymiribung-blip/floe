import { useLayoutEffect } from "react";
import { isOverlayWindow } from "./lib/windowLabel";
import { Overlay } from "./components/overlay/Overlay";
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
    return <Overlay />;
  }
  return <App />;
}
