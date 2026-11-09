// `toast()` from anywhere, shown by Astryx. A message before the frame has mounted waits for it.

import { useToast } from "@astryxdesign/core/Toast";
import { useEffect } from "react";

type Show = (text: string) => void;

let show: Show | null = null;
let waiting: string[] = [];

/** A short message that leaves by itself. */
export function toast(text: string): void {
  if (show) show(text);
  else waiting.push(text);
}

export function ToastBridge() {
  const showToast = useToast();
  useEffect(() => {
    show = (text) => {
      showToast({ body: text, isAutoHide: true, autoHideDuration: 5000 });
    };
    const held = waiting;
    waiting = [];
    for (const text of held) show(text);
    return () => {
      show = null;
    };
  }, [showToast]);
  return null;
}
