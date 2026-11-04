// Which overlay is open: one at a time, read by whoever shows one, without re-rendering the rest.

import { useSyncExternalStore } from "react";

export type OverlayName = "palette" | "ticket" | "give";

/** What a new ticket starts with, when it comes from a message. */
export interface TicketPrefill {
  title: string;
  description: string;
  fromMessage: string;
}

let current: OverlayName | null = null;
let prefill: TicketPrefill | null = null;
const listeners = new Set<() => void>();
const set = (next: OverlayName | null) => {
  current = next;
  for (const listener of listeners) listener();
};

export const overlays = {
  open: (name: OverlayName, start: TicketPrefill | null = null) => {
    prefill = start;
    set(name);
  },
  close: () => set(null),
  prefill: () => prefill,
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
