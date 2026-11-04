// What differs between Electron and the browser. The UI reaches the machine only through this,
// which is what keeps it working on the web.

import type { OpenLink, SessionStore } from "@aurelius/client";
import { createContext, useContext } from "react";

export interface Platform {
  kind: "electron" | "web";
  openLink: OpenLink;
  /** The board member's session on this device. */
  sessions: SessionStore;
  /** Small preferences of this device. May be unavailable, so it never throws. */
  storage: { get(key: string): string | null; set(key: string, value: string): void };
  /** What the sessions list calls this device. */
  deviceName: string;
  version: string;
  /** How a device that is not paired gets paired; null: it never needs to (the local pipe). */
  pair: ((code: string, deviceName: string) => Promise<void>) | null;
}

const PlatformContext = createContext<Platform | null>(null);

export const PlatformProvider = PlatformContext.Provider;

export function usePlatform(): Platform {
  const platform = useContext(PlatformContext);
  if (platform === null) throw new Error("No platform.");
  return platform;
}
