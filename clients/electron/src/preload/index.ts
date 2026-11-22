// The only door between the page and the machine: named calls, no generic IPC, no Node objects.

import { contextBridge, ipcRenderer, webUtils } from "electron";

type FrameListener = (text: string) => void;
type ArtifactBounds = { x: number; y: number; width: number; height: number };
type ClosedListener = (code: number | null) => void;

const listeners = new Map<number, { frame: FrameListener; closed: ClosedListener }>();

ipcRenderer.on("link:frame", (_event, id: number, text: string) => listeners.get(id)?.frame(text));
ipcRenderer.on("link:closed", (_event, id: number, code: number | null) => {
  listeners.get(id)?.closed(code);
  listeners.delete(id);
});

/** Electron puts its own words before an error of the main process; the page shows the message. */
const invoke = (channel: string, ...args: unknown[]) =>
  ipcRenderer.invoke(channel, ...args).catch((error: unknown) => {
    const text = error instanceof Error ? error.message : String(error);
    throw new Error(text.replace(/^Error invoking remote method '[^']*': (?:Error: )?/, ""));
  });

let osLink: (() => void) | null = null;
ipcRenderer.on("links:arrived", () => osLink?.());

contextBridge.exposeInMainWorld("aurelius", {
  listServers: () => invoke("servers:list"),
  useServer: (hostId: string | null) => invoke("servers:use", hostId),
  pairServer: (text: string) => invoke("servers:pair", text),
  cancelPairing: () => invoke("servers:cancel-pair"),
  forgetServer: (hostId: string) => invoke("servers:forget", hostId),
  takeLink: () => invoke("links:take"),
  holdLink: (url: string) => invoke("links:hold", url),
  listenLinks: (arrived: () => void) => {
    osLink = arrived;
  },
  startupAvailable: () => invoke("startup:available"),
  getStartup: () => invoke("startup:get"),
  setStartup: (on: boolean) => invoke("startup:set", on),
  openProjectWindow: (id: string, name: string) => invoke("window:open-project", id, name),
  setTitleBar: (dark: boolean) => invoke("window:title-bar", dark),
  quit: () => invoke("app:quit"),
  openLink: () => invoke("link:open"),
  sendLink: (id: number, text: string) => invoke("link:send", id, text),
  closeLink: (id: number, reason?: string) => {
    listeners.delete(id);
    return invoke("link:close", id, reason);
  },
  listenLink: (id: number, frame: FrameListener, closed: ClosedListener) => {
    listeners.set(id, { frame, closed });
  },
  getSession: () => invoke("session:get"),
  setSession: (token: string | null) => invoke("session:set", token),
  info: () => invoke("app:info"),
  desktopPrefs: () => invoke("app:desktop-prefs"),
  copy: (text: string) => invoke("clipboard:write", text),
  copySecret: (text: string) => invoke("clipboard:write-secret", text),
  saveSecretFile: (name: string, content: string) => invoke("file:save-secret", name, content),
  pickFiles: () => invoke("files:pick"),
  pathOf: (file: File) => webUtils.getPathForFile(file),
  storeFile: (name: string, bytes: ArrayBuffer) => invoke("files:store", name, bytes),
  pickFolder: (title: string) => invoke("folder:pick", title),
  openFolder: (path: string) => invoke("folder:open", path),
  serveArtifact: (html: string) => invoke("artifact:serve", html),
  releaseArtifact: (url: string) => invoke("artifact:release", url),
  showArtifact: (url: string, bounds: ArtifactBounds | null) => invoke("artifact:show", url, bounds),
  placeArtifact: (bounds: ArtifactBounds | null) => invoke("artifact:bounds", bounds),
  hideArtifact: () => invoke("artifact:hide"),
  exportArtifact: (url: string, kind: "pdf" | "png", name: string, width: number) =>
    invoke("artifact:export", url, kind, name, width),
});
