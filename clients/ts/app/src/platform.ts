// What differs between Electron and the browser. The UI reaches the machine only through this,
// which is what keeps it working on the web.

import type { OpenLink, SessionStore } from "@aurelius/client";
import { createContext, useContext } from "react";

/** The servers this device can work with. Changing them makes the app start over. */
export interface ServerChoices {
  /** Null: this computer's own service. */
  active: string | null;
  localName: string | null;
  localAvailable: boolean;
  servers: { hostId: string; hostName: string; url: string; lastUsedMs: number }[];
}

/** Where the artifact view lies, in the page's CSS pixels from the top left of the window. */
export interface ArtifactBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Which view of the viewer: its own, or the older version beside it while it compares. */
export type ArtifactSlot = "main" | "compare";

/** The element the board member clicked in comment mode, as the desktop checked it. */
export interface ArtifactPick {
  selector: string;
  tag: string;
  /** Its text on one line, at most 120 characters. */
  text: string;
  rect: ArtifactBounds;
}

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
  /** Writes text. A failure rejects, so a caller can say so. */
  copy(text: string): Promise<void>;
  /** Writes a secret and clears it after 60 s if it is still there. `sure`: this device can promise that. */
  copySecret(text: string): Promise<{ sure: boolean }>;
  /** Stores a secret so only this user can read it; a browser can only offer a download. Null: cancelled. */
  saveSecretFile(name: string, content: string): Promise<{ path: string | null; private: boolean } | null>;
  /**
   * Chooses files for a ticket and gives the paths the server can read: where the server is on
   * this machine the chosen paths, otherwise they are uploaded first. Empty: cancelled.
   */
  pickFiles(): Promise<{ path: string; name: string }[]>;
  /** The same for files that were pasted or dropped. */
  keepFiles(files: File[]): Promise<{ path: string; name: string }[]>;
  /** The folder picker of this machine; null where there is none and the path must be typed. */
  pickFolder: ((title: string) => Promise<string | null>) | null;
  /** Opens a folder of this machine; null where a folder cannot be opened. */
  openFolder: ((path: string) => Promise<void>) | null;
  /** The paired server this page must find on the other side; null: its own service or origin. */
  target: { hostId: string; environment: string } | null;
  /** Null: this page belongs to one server. */
  servers: {
    list(): Promise<ServerChoices>;
    use(hostId: string | null): Promise<void>;
    /** Waits for the person at the server; `cancelled`: `cancelPairing` ended it. */
    pair(text: string): Promise<{ cancelled: boolean }>;
    cancelPairing(): Promise<void>;
    forget(hostId: string): Promise<void>;
  } | null;
  /** `aurelius://` from the operating system; null: a browser. */
  osLinks: {
    take(): Promise<string | null>;
    /** Keeps a link for after the app started over on another server. */
    hold(url: string): Promise<void>;
    listen(arrived: () => void): void;
  } | null;
  /** The service starting when this user signs in; null where this install can't. */
  startup: { get(): Promise<boolean>; set(on: boolean): Promise<boolean> } | null;
  /** The project this window was opened for, if it isn't the main window. */
  windowProject: string | null;
  openProjectWindow: ((id: string, name: string) => Promise<void>) | null;
  /**
   * Design artifacts (kept HTML files) run here, in a view of their own without network that the
   * desktop lays over the page. Null where there is no safe place to run them, a browser: there an
   * HTML file is saved, like any other file.
   */
  artifacts: {
    /** Hands the HTML over and gives its URL; `release` lets go of it. */
    serve(html: string): Promise<string>;
    release(url: string): Promise<void>;
    /**
     * Shows a served artifact over the page, at `bounds` (the stage, in the page's CSS pixels);
     * `place` follows the stage, null hides it for now; `hide` takes the view away.
     */
    show(url: string, bounds: ArtifactBounds | null, slot?: ArtifactSlot): Promise<void>;
    place(bounds: ArtifactBounds | null, slot?: ArtifactSlot): Promise<void>;
    /** Without a slot: every view of this page. */
    hide(slot?: ArtifactSlot): Promise<void>;
    /**
     * Shows newer HTML of the design in the viewer's own view, in the same document: no new view,
     * so no flash and the scroll stays. False when there is no view to show it in.
     */
    update(html: string): Promise<boolean>;
    /**
     * Comment mode in the viewer's own view: resolves with the element the board member clicks,
     * or null when `stopPick` ended it or the view went.
     */
    pick(): Promise<ArtifactPick | null>;
    stopPick(): Promise<void>;
    /** Asks where to save and makes the file; null: cancelled. */
    export(url: string, kind: "pdf" | "png", name: string, width: number): Promise<string | null>;
  } | null;
  /** Ends the app; the service keeps running. Null: a browser tab can't close itself. */
  quit: (() => void) | null;
}

const PlatformContext = createContext<Platform | null>(null);

export const PlatformProvider = PlatformContext.Provider;

export function usePlatform(): Platform {
  const platform = useContext(PlatformContext);
  if (platform === null) throw new Error("No platform.");
  return platform;
}
