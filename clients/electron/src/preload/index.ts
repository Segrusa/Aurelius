// The only door between the page and the machine: named calls, no generic IPC, no Node objects.

import { contextBridge, ipcRenderer } from "electron";

type FrameListener = (text: string) => void;
type ClosedListener = (code: number | null) => void;

const listeners = new Map<number, { frame: FrameListener; closed: ClosedListener }>();

ipcRenderer.on("link:frame", (_event, id: number, text: string) => listeners.get(id)?.frame(text));
ipcRenderer.on("link:closed", (_event, id: number, code: number | null) => {
  listeners.get(id)?.closed(code);
  listeners.delete(id);
});

contextBridge.exposeInMainWorld("aurelius", {
  openLink: () => ipcRenderer.invoke("link:open"),
  sendLink: (id: number, text: string) => ipcRenderer.invoke("link:send", id, text),
  closeLink: (id: number) => {
    listeners.delete(id);
    return ipcRenderer.invoke("link:close", id);
  },
  listenLink: (id: number, frame: FrameListener, closed: ClosedListener) => {
    listeners.set(id, { frame, closed });
  },
  getSession: () => ipcRenderer.invoke("session:get"),
  setSession: (token: string | null) => ipcRenderer.invoke("session:set", token),
  info: () => ipcRenderer.invoke("app:info"),
  desktopPrefs: () => ipcRenderer.invoke("app:desktop-prefs"),
  copy: (text: string) => ipcRenderer.invoke("clipboard:write", text),
  copySecret: (text: string) => ipcRenderer.invoke("clipboard:write-secret", text),
  saveSecretFile: (name: string, content: string) => ipcRenderer.invoke("file:save-secret", name, content),
  pickFiles: () => ipcRenderer.invoke("files:pick"),
  pickFolder: (title: string) => ipcRenderer.invoke("folder:pick", title),
  openFolder: (path: string) => ipcRenderer.invoke("folder:open", path),
});
