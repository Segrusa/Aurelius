// Where artifacts run: a session of their own, in memory, that reaches nothing. CSP and the sandbox
// are not a network boundary in Chromium; an artifact in the app's own session got out through
// WebRTC, DNS prefetch and speculation rules (Sentinel, AUR-T79-SEC1). A session is per
// WebContents, so the viewer is a WebContentsView of its own that main lays over the page's stage,
// and the export window uses the same session.

import { BrowserWindow, type Session, session, type WebContents, WebContentsView } from "electron";
import {
  ARTIFACT_CSP,
  ARTIFACT_PARTITION,
  ARTIFACT_PROXY,
  ARTIFACT_SCHEME,
  type ArtifactStore,
  artifactMayNavigate,
  artifactRequestAllowed,
  EXPORT_HEIGHT_DEFAULT,
  EXPORT_TIMEOUT_MESSAGE,
  EXPORT_TIMEOUT_MS,
  EXPORT_WORLD_ID,
  viewBounds,
  withinTime,
} from "./artifact";

/** The same preferences for every WebContents that shows an artifact: no preload, no Node. */
const preferences = (artifactSession: Session): Electron.WebPreferences => ({
  session: artifactSession,
  sandbox: true,
  contextIsolation: true,
  nodeIntegration: false,
  webSecurity: true,
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
    headers.set("Content-Type", "text/html; charset=utf-8");
    return new Response(html, { status: 200, headers });
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

interface Shown {
  view: WebContentsView;
  window: BrowserWindow;
  detach: () => void;
}

/**
 * The viewers that are open, at most one per page (the WebContents that asked). A view follows only
 * the page that showed it, and goes as soon as that page navigates, reloads, crashes or closes.
 */
export class ArtifactViews {
  private readonly shown = new Map<number, Shown>();
  /** Counts the shows of each page, so a show that a later show or hide overtook does nothing. */
  private readonly asked = new Map<number, number>();

  constructor(
    private readonly store: ArtifactStore,
    private readonly ready: Promise<Session>,
  ) {}

  /** Lays a new view with `url` over `owner`'s stage; a view the page showed before goes first. */
  async show(owner: WebContents, url: string, bounds: unknown): Promise<void> {
    if (!this.store.owns(owner.id, url)) throw new Error("This artifact is not open any more.");
    const window = BrowserWindow.fromWebContents(owner);
    if (window === null) throw new Error("No window to show this artifact in.");
    this.hide(owner.id);
    const ask = (this.asked.get(owner.id) ?? 0) + 1;
    this.asked.set(owner.id, ask);
    const artifactSession = await this.ready;
    if (this.asked.get(owner.id) !== ask || window.isDestroyed() || owner.isDestroyed()) return;

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
    this.shown.set(owner.id, { view, window, detach });
    window.contentView.addChildView(view);
    this.place(owner, bounds);
    view.webContents.loadURL(url).catch(() => undefined);
  }

  /** Moves `owner`'s view to where its stage is now; null bounds hide it. Only its own view. */
  place(owner: WebContents, bounds: unknown): void {
    const shown = this.shown.get(owner.id);
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

  /** Takes `owner`'s view away and ends its renderer. */
  hide(ownerId: number): void {
    this.asked.set(ownerId, (this.asked.get(ownerId) ?? 0) + 1);
    const shown = this.shown.get(ownerId);
    if (shown === undefined) return;
    this.shown.delete(ownerId);
    shown.detach();
    if (!shown.window.isDestroyed()) shown.window.contentView.removeChildView(shown.view);
    if (!shown.view.webContents.isDestroyed()) shown.view.webContents.close();
  }

  /** How many views are open: for the probe and for a check that none stays behind. */
  get size(): number {
    return this.shown.size;
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
