// Where artifacts run: a session of their own, in memory, that reaches nothing. CSP and the sandbox
// are not a network boundary in Chromium; an artifact in the app's own session got out through
// WebRTC, DNS prefetch and speculation rules (Sentinel, AUR-T79-SEC1). A session is per
// WebContents, so the viewer is a WebContentsView of its own that main lays over the page's stage,
// and the export window uses the same session.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BrowserWindow, type Session, session, type WebContents, WebContentsView } from "electron";
import {
  ARTIFACT_BYTES_MAX,
  ARTIFACT_CSP,
  ARTIFACT_PARTITION,
  ARTIFACT_PROXY,
  ARTIFACT_SCHEME,
  type ArtifactStore,
  artifactMayNavigate,
  artifactNonce,
  artifactRequestAllowed,
  artifactResponse,
  EXPORT_HEIGHT_DEFAULT,
  EXPORT_TIMEOUT_MESSAGE,
  EXPORT_TIMEOUT_MS,
  EXPORT_WORLD_ID,
  resultWithin,
  viewBounds,
  WORLD_CALL_MS,
  withinTime,
} from "./artifact";
import {
  type ArtifactPick,
  type ArtifactTweak,
  checkPick,
  currentValue,
  PICK_SELECTOR_MAX,
  PICK_TEXT_MAX,
  parseTweaks,
  TWEAKS_ELEMENT_ID,
  TWEAKS_JSON_MAX,
  tweakValue,
} from "./artifact-tweaks";
import {
  morphChildren,
  morphInWorld,
  pickInWorld,
  readTweaksInWorld,
  resetTweaksInWorld,
  selectorOf,
  setTweaksInWorld,
  stopPickInWorld,
  TOOLS_WORLD_ID,
  valuesInWorld,
  worldCall,
} from "./artifact-world";

/**
 * The artifact's preload (src/preload/artifact.ts): no bridge and no IPC; it only takes WebRTC out
 * of every frame, because Chromium resolves a TURN server's host name even through the dead proxy.
 */
const ARTIFACT_PRELOAD = join(dirname(fileURLToPath(import.meta.url)), "../preload/artifact.cjs");

/** The same preferences for every WebContents that shows an artifact: no Node, no bridge. */
const preferences = (artifactSession: Session): Electron.WebPreferences => ({
  session: artifactSession,
  sandbox: true,
  contextIsolation: true,
  nodeIntegration: false,
  webSecurity: true,
  preload: ARTIFACT_PRELOAD,
  // In child frames too: an artifact's srcdoc frame runs code of its own.
  nodeIntegrationInSubFrames: true,
});

/**
 * Makes the artifact session: it serves the store on `artifact:`, cancels every other request,
 * sends what slips past to a proxy that is not there, and grants no permission. Resolves once the
 * proxy is set; nothing may load in it before that.
 */
export async function artifactSession(store: ArtifactStore): Promise<Session> {
  const artifactSession = session.fromPartition(ARTIFACT_PARTITION);
  artifactSession.protocol.handle(ARTIFACT_SCHEME, (request) => {
    const html = store.html(request.url);
    const headers = new Headers({
      "Content-Security-Policy": ARTIFACT_CSP,
      "X-Content-Type-Options": "nosniff",
      "Cache-Control": "no-store",
    });
    if (html === null || request.method !== "GET") return new Response("Not found", { status: 404, headers });
    // A fresh nonce for every response, a reload too.
    const { body, csp } = artifactResponse(html, artifactNonce());
    headers.set("Content-Security-Policy", csp);
    headers.set("Content-Type", "text/html; charset=utf-8");
    return new Response(body, { status: 200, headers });
  });
  artifactSession.webRequest.onBeforeRequest((details, callback) =>
    callback({ cancel: !artifactRequestAllowed(details.url) }),
  );
  artifactSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  artifactSession.setPermissionCheckHandler(() => false);
  artifactSession.setDevicePermissionHandler(() => false);
  artifactSession.on("will-download", (event) => event.preventDefault());
  await artifactSession.setProxy({ ...ARTIFACT_PROXY });
  return artifactSession;
}

/**
 * The rules of every WebContents that shows `own`: no UDP outside the (dead) proxy for WebRTC, no
 * window of its own, and no navigation in any frame away from its own document.
 */
export function guardArtifact(contents: WebContents, own: string): void {
  contents.setWebRTCIPHandlingPolicy("disable_non_proxied_udp");
  contents.setWindowOpenHandler(() => ({ action: "deny" }));
  contents.on("will-navigate", (event, url) => {
    if (!artifactMayNavigate(own, url)) event.preventDefault();
  });
  contents.on("will-redirect", (event, url) => {
    if (!artifactMayNavigate(own, url)) event.preventDefault();
  });
  contents.on("will-frame-navigate", (details) => {
    if (!artifactMayNavigate(own, details.url)) details.preventDefault();
  });
}

/**
 * Which of a page's views: the viewer's own, and the older version beside it while it compares.
 * Comments and tweaks work only in the viewer's own.
 */
export type ViewSlot = "main" | "compare";

/** The slot a page named; anything else is the viewer's own. */
export function viewSlot(raw: unknown): ViewSlot {
  return raw === "compare" ? "compare" : "main";
}

/** A tweak as the page gets it: what main checked, with its value now. */
export type ShownTweak = ArtifactTweak & { value: string };

interface Shown {
  view: WebContentsView;
  window: BrowserWindow;
  detach: () => void;
  /** Ends a pick that is waiting, with null. */
  stopPick: (() => void) | null;
  /** The tweaks main read from this view's artifact: the only variables it sets in it. */
  tweaks: ArtifactTweak[];
}

const key = (ownerId: number, slot: ViewSlot) => `${ownerId}/${slot}`;
const SLOTS: readonly ViewSlot[] = ["main", "compare"];

/**
 * The viewers that are open, at most one per page (the WebContents that asked) and slot. A view
 * follows only the page that showed it, and goes as soon as that page navigates, reloads, crashes
 * or closes.
 */
export class ArtifactViews {
  private readonly shown = new Map<string, Shown>();
  /** Counts the shows of each page and slot, so a show that a later show or hide overtook does nothing. */
  private readonly asked = new Map<string, number>();

  constructor(
    private readonly store: ArtifactStore,
    private readonly ready: Promise<Session>,
  ) {}

  /** Lays a new view with `url` over `owner`'s stage; a view the page showed before in that slot goes first. */
  async show(owner: WebContents, url: string, bounds: unknown, slot: ViewSlot = "main"): Promise<void> {
    if (!this.store.owns(owner.id, url)) throw new Error("This artifact is not open any more.");
    const window = BrowserWindow.fromWebContents(owner);
    if (window === null) throw new Error("No window to show this artifact in.");
    this.hide(owner.id, slot);
    const at = key(owner.id, slot);
    const ask = (this.asked.get(at) ?? 0) + 1;
    this.asked.set(at, ask);
    const artifactSession = await this.ready;
    if (this.asked.get(at) !== ask || window.isDestroyed() || owner.isDestroyed()) return;

    const view = new WebContentsView({ webPreferences: preferences(artifactSession) });
    // An artifact paints its own page; without a background of its own it is a white one.
    view.setBackgroundColor("#FFFFFF");
    guardArtifact(view.webContents, url);
    const gone = () => this.hide(owner.id);
    owner.on("did-navigate", gone);
    owner.on("did-navigate-in-page", gone);
    owner.on("render-process-gone", gone);
    owner.on("destroyed", gone);
    window.on("closed", gone);
    const detach = () => {
      owner.off("did-navigate", gone);
      owner.off("did-navigate-in-page", gone);
      owner.off("render-process-gone", gone);
      owner.off("destroyed", gone);
      window.off("closed", gone);
    };
    this.shown.set(at, { view, window, detach, stopPick: null, tweaks: [] });
    window.contentView.addChildView(view);
    this.place(owner, bounds, slot);
    view.webContents.loadURL(url).catch(() => undefined);
  }

  /** Moves `owner`'s view to where its stage is now; null bounds hide it. Only its own view. */
  place(owner: WebContents, bounds: unknown, slot: ViewSlot = "main"): void {
    const shown = this.shown.get(key(owner.id, slot));
    if (shown === undefined || shown.window.isDestroyed()) return;
    const content = shown.window.getContentBounds();
    const placed = viewBounds(bounds, owner.getZoomFactor(), content);
    if (placed === null) {
      shown.view.setVisible(false);
      return;
    }
    shown.view.setBounds(placed);
    shown.view.setVisible(true);
  }

  /** Takes `owner`'s view in `slot` away (all of its views without one) and ends its renderer. */
  hide(ownerId: number, slot?: ViewSlot): void {
    for (const one of slot === undefined ? SLOTS : [slot]) {
      const at = key(ownerId, one);
      this.asked.set(at, (this.asked.get(at) ?? 0) + 1);
      const shown = this.shown.get(at);
      if (shown === undefined) continue;
      this.shown.delete(at);
      shown.stopPick?.();
      shown.detach();
      if (!shown.window.isDestroyed()) shown.window.contentView.removeChildView(shown.view);
      if (!shown.view.webContents.isDestroyed()) shown.view.webContents.close();
    }
  }

  /** How many views are open: for the probe and for a check that none stays behind. */
  get size(): number {
    return this.shown.size;
  }

  /** The viewer's own view of `owner`, when it is there and loaded far enough to run code in. */
  private own(owner: WebContents): Shown | null {
    const shown = this.shown.get(key(owner.id, "main"));
    return shown === undefined || shown.view.webContents.isDestroyed() ? null : shown;
  }

  /**
   * Runs `code` in the tools world of a view; a page that went, a renderer that failed or a call
   * over WORLD_CALL_MS gives null. Only a pick (`limited` false) waits as long as it takes.
   */
  private run(shown: Shown, code: string, limited = true): Promise<unknown> {
    const work = this.runNow(shown, code);
    return limited ? resultWithin(work, WORLD_CALL_MS) : work;
  }

  private async runNow(shown: Shown, code: string): Promise<unknown> {
    const contents = shown.view.webContents;
    // A view the page just showed may still load its document; the tools work on the loaded one.
    if (contents.isLoadingMainFrame())
      await new Promise<void>((resolve) => {
        contents.once("did-stop-loading", () => resolve());
        contents.once("destroyed", () => resolve());
      });
    if (contents.isDestroyed()) return null;
    return contents.executeJavaScriptInIsolatedWorld(TOOLS_WORLD_ID, [{ code }]).catch(() => null);
  }

  /**
   * Comment mode: waits for the board member to click an element in `owner`'s view, and gives it
   * as checked by checkPick. Null when the mode was stopped, the view went, or the result is not a
   * pick. A new pick ends one that is still waiting.
   */
  async pick(owner: WebContents): Promise<ArtifactPick | null> {
    const shown = this.own(owner);
    if (shown === null) return null;
    shown.stopPick?.();
    const stopped = new Promise<null>((resolve) => {
      shown.stopPick = () => resolve(null);
    });
    const picked = this.run(
      shown,
      worldCall(pickInWorld, { selectorMax: PICK_SELECTOR_MAX, textMax: PICK_TEXT_MAX }, selectorOf),
      false,
    );
    const raw = await Promise.race([picked, stopped]);
    return checkPick(raw);
  }

  /**
   * Shows `html` in `owner`'s view in place of what it shows, in the same document (morphInWorld):
   * a design that is still being written grows without a new view. False when there is no view,
   * the HTML is not a string within ARTIFACT_BYTES_MAX, or the world call failed.
   */
  async update(owner: WebContents, html: unknown): Promise<boolean> {
    if (typeof html !== "string" || html.length > ARTIFACT_BYTES_MAX) return false;
    const shown = this.own(owner);
    if (shown === null) return false;
    return (await this.run(shown, worldCall(morphInWorld, html, morphChildren))) === true;
  }

  /** Ends comment mode in `owner`'s view: the outline goes, and a waiting pick gives null. */
  async stopPick(owner: WebContents): Promise<void> {
    const shown = this.own(owner);
    if (shown === null) return;
    shown.stopPick?.();
    shown.stopPick = null;
    await this.run(shown, worldCall(stopPickInWorld));
  }

  /** The tweaks the artifact in `owner`'s view declares, checked, each with its computed value. */
  async tweaks(owner: WebContents): Promise<ShownTweak[]> {
    const shown = this.own(owner);
    if (shown === null) return [];
    const tweaks = parseTweaks(await this.run(shown, worldCall(readTweaksInWorld, TWEAKS_ELEMENT_ID, TWEAKS_JSON_MAX)));
    shown.tweaks = tweaks;
    if (tweaks.length === 0) return [];
    const values = await this.run(
      shown,
      worldCall(
        valuesInWorld,
        tweaks.map((t) => t.var),
      ),
    );
    return tweaks.map((t, i) => ({ ...t, value: currentValue(Array.isArray(values) ? values[i] : null) }));
  }

  /**
   * Sets one tweak in `owner`'s view: only a variable main read from it, with a value tweakValue
   * takes. Gives the value set, or null when it was refused or the world did not set it in time.
   */
  async tweak(owner: WebContents, name: unknown, value: unknown): Promise<string | null> {
    const shown = this.own(owner);
    const tweak = shown?.tweaks.find((t) => t.var === name);
    if (shown === null || tweak === undefined) return null;
    const checked = tweakValue(tweak, value);
    if (checked === null) return null;
    const set = await this.run(shown, worldCall(setTweaksInWorld, [[tweak.var, checked]]));
    return set === true ? checked : null;
  }

  /** Puts back what the tweaks set in `owner`'s view. */
  async resetTweaks(owner: WebContents): Promise<void> {
    const shown = this.own(owner);
    if (shown !== null) await this.run(shown, worldCall(resetTweaksInWorld));
  }
}

/**
 * Loads an artifact in a window nobody sees, in the artifact session, to print or capture it.
 * It may not go anywhere else or open anything. Load and `make` together get EXPORT_TIMEOUT_MS;
 * the window is always closed afterwards, its renderer ended first (a busy one does not stop).
 */
/** The artifact's page height, read in a world of main's own and without a user gesture. */
export function pageHeight(contents: WebContents): Promise<unknown> {
  return contents.executeJavaScriptInIsolatedWorld(EXPORT_WORLD_ID, [
    { code: "document.documentElement.scrollHeight" },
  ]);
}

export async function renderArtifact<R>(
  ready: Promise<Session>,
  url: string,
  width: number,
  make: (window: BrowserWindow) => Promise<R>,
): Promise<R> {
  const window = new BrowserWindow({
    show: false,
    width,
    height: EXPORT_HEIGHT_DEFAULT,
    useContentSize: true,
    backgroundColor: "#FFFFFF",
    webPreferences: preferences(await ready),
  });
  const contents = window.webContents;
  guardArtifact(contents, url);
  window.setContentSize(width, EXPORT_HEIGHT_DEFAULT);
  const work = async () => {
    // A navigation the artifact tries (and that is refused) can end the load early; the page is there.
    await window.loadURL(url).catch((error: unknown) => {
      if (contents.getURL() !== url) throw error;
    });
    return make(window);
  };
  try {
    return await withinTime(work(), EXPORT_TIMEOUT_MS, EXPORT_TIMEOUT_MESSAGE);
  } finally {
    if (!contents.isDestroyed()) contents.forcefullyCrashRenderer();
    window.destroy();
  }
}
