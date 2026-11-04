import { App, type Platform } from "@aurelius/app";
import { pairBrowser, webLink } from "@aurelius/client";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

const SESSION_KEY = "aurelius.session";

function safe<T>(action: () => T, fallback: T): T {
  try {
    return action();
  } catch {
    return fallback;
  }
}

/** To the server's inbox for this device; a ticket then names them by that path. */
async function uploadFiles(files: File[]): Promise<{ path: string; name: string }[]> {
  const uploaded: { path: string; name: string }[] = [];
  for (const file of files) {
    const body = new FormData();
    body.append("file", file);
    const response = await fetch("/v1/upload", { method: "POST", body });
    if (!response.ok) throw new Error(`${file.name} did not upload.`);
    uploaded.push((await response.json()) as { path: string; name: string });
  }
  return uploaded;
}

const platform: Platform = {
  kind: "web",
  openLink: webLink(location.origin),
  // Scripts can read this, so it lasts only as long as this tab. The device's own credentials stay
  // in `HttpOnly` cookies, out of their reach.
  sessions: {
    get: async () => safe(() => sessionStorage.getItem(SESSION_KEY), null),
    set: async (token) => {
      safe(
        () => (token === null ? sessionStorage.removeItem(SESSION_KEY) : sessionStorage.setItem(SESSION_KEY, token)),
        undefined,
      );
    },
  },
  storage: {
    get: (key) => safe(() => localStorage.getItem(`aurelius.${key}`), null),
    set: (key, value) => safe(() => localStorage.setItem(`aurelius.${key}`, value), undefined),
  },
  deviceName: "Browser",
  version: import.meta.env.VITE_AURELIUS_VERSION ?? "0.0.0",
  pair: (code, name) => pairBrowser(location.origin, code, name),
  copy: (text) => navigator.clipboard.writeText(text),
  // A browser only lets a page read the clipboard while focused and allowed, so this is best effort.
  copySecret: async (text) => {
    await navigator.clipboard.writeText(text);
    setTimeout(() => {
      navigator.clipboard
        .readText()
        .then((now) => (now === text ? navigator.clipboard.writeText("") : undefined))
        .catch(() => undefined);
    }, 60_000);
    return { sure: false };
  },
  saveSecretFile: async (name, content) => {
    const link = document.createElement("a");
    const url = URL.createObjectURL(new Blob([`${content}\n`], { type: "text/plain" }));
    link.href = url;
    link.download = name;
    link.click();
    URL.revokeObjectURL(url);
    return { path: null, private: false };
  },
  pickFiles: () =>
    new Promise((resolve, reject) => {
      const input = document.createElement("input");
      input.type = "file";
      input.multiple = true;
      input.addEventListener("cancel", () => resolve([]));
      input.addEventListener("change", () => {
        uploadFiles([...(input.files ?? [])]).then(resolve, reject);
      });
      input.click();
    }),
  keepFiles: uploadFiles,
  pickFolder: null,
  openFolder: null,
  target: null,
  servers: null,
  osLinks: null,
  startup: null,
  windowProject: null,
  openProjectWindow: null,
  quit: null,
};

const root = document.getElementById("root");
if (root === null) throw new Error("No root element.");
createRoot(root).render(
  <StrictMode>
    <App platform={platform} />
  </StrictMode>,
);
