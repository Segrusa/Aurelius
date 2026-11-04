// Which overlay is open: one at a time, read by whoever shows one, without re-rendering the rest.

import { useSyncExternalStore } from "react";

export type OverlayName = "palette" | "ticket" | "give";

let current: OverlayName | null = null;
const listeners = new Set<() => void>();
const set = (next: OverlayName | null) => {
  current = next;
  for (const listener of listeners) listener();
};

export const overlays = {
  open: (name: OverlayName) => set(name),
  close: () => set(null),
};

export function useOpenOverlay(): OverlayName | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}
