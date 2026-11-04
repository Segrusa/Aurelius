import { App, adoptDesktopPrefs, type DesktopPrefs, type Platform } from "@aurelius/app";
import { LinkError } from "@aurelius/client";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

/** What the preload exposes. */
interface Bridge {
  openLink(): Promise<{ ok: true; id: number } | { ok: false; message: string }>;
  sendLink(id: number, text: string): Promise<void>;
  closeLink(id: number): Promise<void>;
  listenLink(id: number, frame: (text: string) => void, closed: (code: number | null) => void): void;
  getSession(): Promise<string | null>;
  setSession(token: string | null): Promise<void>;
  info(): Promise<{ version: string; deviceName: string }>;
  desktopPrefs(): Promise<DesktopPrefs>;
  copy(text: string): Promise<void>;
  copySecret(text: string): Promise<{ sure: boolean }>;
  saveSecretFile(name: string, content: string): Promise<{ path: string | null; private: boolean } | null>;
  pickFiles(): Promise<string[]>;
  pickFolder(title: string): Promise<string | null>;
  openFolder(path: string): Promise<void>;
}

const bridge = (window as unknown as { aurelius: Bridge }).aurelius;

function safe<T>(action: () => T, fallback: T): T {
  try {
    return action();
  } catch {
    return fallback;
  }
}

const [info, desktopPrefs] = await Promise.all([bridge.info(), bridge.desktopPrefs()]);

const platform: Platform = {
  kind: "electron",
  openLink: async () => {
    const opened = await bridge.openLink();
    if (!opened.ok) throw new LinkError("unreachable", opened.message);
    const { id } = opened;
    return {
      send: (text) => void bridge.sendLink(id, text),
      close: () => void bridge.closeLink(id),
      listen: (onFrame, onClosed) => bridge.listenLink(id, onFrame, onClosed),
    };
  },
  sessions: { get: () => bridge.getSession(), set: (token) => bridge.setSession(token) },
  storage: {
    get: (key) => safe(() => localStorage.getItem(`aurelius.${key}`), null),
    set: (key, value) => safe(() => localStorage.setItem(`aurelius.${key}`, value), undefined),
  },
  deviceName: info.deviceName,
  version: info.version,
  pair: null,
  copy: (text) => bridge.copy(text),
  copySecret: (text) => bridge.copySecret(text),
  saveSecretFile: (name, content) => bridge.saveSecretFile(name, content),
  pickFiles: async () => (await bridge.pickFiles()).map((path) => ({ path, name: path.split(/[\\/]/).pop() ?? path })),
  pickFolder: (title) => bridge.pickFolder(title),
  openFolder: (path) => bridge.openFolder(path),
};

adoptDesktopPrefs(platform.storage, desktopPrefs);

const root = document.getElementById("root");
if (root === null) throw new Error("No root element.");
createRoot(root).render(
  <StrictMode>
    <App platform={platform} />
  </StrictMode>,
);
