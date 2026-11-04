import { App, adoptDesktopPrefs, type DesktopPrefs, type Platform, type ServerChoices } from "@aurelius/app";
import { LinkError } from "@aurelius/client";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

/** What the preload exposes. */
interface Bridge {
  openLink(): Promise<{ ok: true; id: number } | { ok: false; kind: LinkError["kind"]; message: string }>;
  sendLink(id: number, text: string): Promise<void>;
  closeLink(id: number): Promise<void>;
  listenLink(id: number, frame: (text: string) => void, closed: (code: number | null) => void): void;
  getSession(): Promise<string | null>;
  setSession(token: string | null): Promise<void>;
  info(): Promise<{
    version: string;
    deviceName: string;
    os: string;
    target: { hostId: string; environment: string } | null;
    windowProject: string | null;
  }>;
  listServers(): Promise<ServerChoices>;
  useServer(hostId: string | null): Promise<void>;
  pairServer(text: string): Promise<{ cancelled: boolean }>;
  cancelPairing(): Promise<void>;
  forgetServer(hostId: string): Promise<void>;
  takeLink(): Promise<string | null>;
  holdLink(url: string): Promise<void>;
  listenLinks(arrived: () => void): void;
  startupAvailable(): Promise<boolean>;
  getStartup(): Promise<boolean>;
  setStartup(on: boolean): Promise<boolean>;
  openProjectWindow(id: string, name: string): Promise<void>;
  setTitleBar(dark: boolean): Promise<void>;
  quit(): Promise<void>;
  desktopPrefs(): Promise<DesktopPrefs>;
  copy(text: string): Promise<void>;
  copySecret(text: string): Promise<{ sure: boolean }>;
  saveSecretFile(name: string, content: string): Promise<{ path: string | null; private: boolean } | null>;
  pickFiles(): Promise<{ path: string; name: string }[]>;
  pathOf(file: File): string;
  storeFile(name: string, bytes: ArrayBuffer): Promise<{ path: string; name: string }>;
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

const [info, desktopPrefs, canStart] = await Promise.all([
  bridge.info(),
  bridge.desktopPrefs(),
  bridge.startupAvailable(),
]);
const remote = info.target !== null;

// The window controls are drawn over the page here; the page leaves them room (style.css).
document.documentElement.dataset.os = info.os;
if (info.os === "win32") {
  const follow = () => {
    const theme = document.documentElement.dataset.theme;
    const dark = theme ? theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
    void bridge.setTitleBar(dark);
  };
  new MutationObserver(follow).observe(document.documentElement, { attributeFilter: ["data-theme"] });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", follow);
  follow();
}

const platform: Platform = {
  kind: "electron",
  openLink: async () => {
    const opened = await bridge.openLink();
    if (!opened.ok) throw new LinkError(opened.kind, opened.message);
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
  pickFiles: () => bridge.pickFiles(),
  keepFiles: async (files) => {
    const kept = [];
    for (const file of files) {
      const path = bridge.pathOf(file);
      // A dropped file on this computer is read where it is; anything else needs a copy.
      kept.push(
        path && !remote ? { path, name: file.name } : await bridge.storeFile(file.name, await file.arrayBuffer()),
      );
    }
    return kept;
  },
  // A folder of this computer means nothing to a server elsewhere: there the server's own browser.
  pickFolder: remote ? null : (title) => bridge.pickFolder(title),
  openFolder: remote ? null : (path) => bridge.openFolder(path),
  target: info.target,
  servers: {
    list: () => bridge.listServers(),
    use: (hostId) => bridge.useServer(hostId),
    pair: (text) => bridge.pairServer(text),
    cancelPairing: () => bridge.cancelPairing(),
    forget: (hostId) => bridge.forgetServer(hostId),
  },
  osLinks: {
    take: () => bridge.takeLink(),
    hold: (url) => bridge.holdLink(url),
    listen: (arrived) => bridge.listenLinks(arrived),
  },
  startup: canStart ? { get: () => bridge.getStartup(), set: (on) => bridge.setStartup(on) } : null,
  windowProject: info.windowProject,
  openProjectWindow: (id, name) => bridge.openProjectWindow(id, name),
  quit: () => void bridge.quit(),
};

adoptDesktopPrefs(platform.storage, desktopPrefs);

const root = document.getElementById("root");
if (root === null) throw new Error("No root element.");
createRoot(root).render(
  <StrictMode>
    <App platform={platform} />
  </StrictMode>,
);
