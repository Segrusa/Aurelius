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
};

const root = document.getElementById("root");
if (root === null) throw new Error("No root element.");
createRoot(root).render(
  <StrictMode>
    <App platform={platform} />
  </StrictMode>,
);
