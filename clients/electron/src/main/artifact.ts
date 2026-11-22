// A design artifact: one HTML file a run kept with its message. The page hands it here, and main
// serves it on `artifact://view/<token>` in a session of its own that has no network (see
// artifact-session.ts). The policy below lets it run its own inline code and nothing else, but CSP
// and the sandbox are not a network boundary in Chromium (WebRTC, DNS prefetch and speculation
// rules get past them): the session is.

import { randomBytes } from "node:crypto";

/** The scheme main serves artifacts on, only in the artifact session. */
export const ARTIFACT_SCHEME = "artifact";
const ARTIFACT_PREFIX = `${ARTIFACT_SCHEME}://view/`;

/** The session every artifact runs in: in memory (no `persist:`), apart from the app's own. */
export const ARTIFACT_PARTITION = "artifact";
/**
 * A proxy that is not there (the discard port), for every scheme, loopback included: what slips
 * past the request filter (WebRTC's TURN over TCP does) reaches nothing.
 */
export const ARTIFACT_PROXY = {
  proxyRules: "http=127.0.0.1:9;https=127.0.0.1:9;socks=127.0.0.1:9",
  proxyBypassRules: "<-loopback>",
} as const;

/**
 * The policy of every artifact response, apart from its scripts (see artifactResponse). Nothing can
 * be fetched or sent. `sandbox` makes it a sandbox of its own also where it is not in the viewer's
 * frame (the export window): an opaque origin without storage, popups, forms or navigation of the top.
 */
export const ARTIFACT_CSP =
  "sandbox allow-scripts; default-src 'none'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'";

/** What main sends for one artifact request: the HTML with its scripts marked, and the policy. */
export interface ArtifactResponse {
  body: string;
  csp: string;
}

/**
 * The artifact with every `<script` element given `nonce`, and a policy that runs only those and
 * `on…=` attributes. Without `'unsafe-inline'` for script elements Chromium refuses `javascript:`
 * URLs, and a `javascript:` frame is an about:blank document the artifact preload does not reach:
 * it got WebRTC back and resolved a TURN host name the artifact chose (Sentinel, AUR-T79-SEC2 N1).
 * The nonce must be fresh for every response, so a page cannot know it before it is served.
 */
export function artifactResponse(html: string, nonce: string): ArtifactResponse {
  if (!/^[0-9a-f]{16,}$/.test(nonce)) throw new Error("An artifact nonce is hex.");
  return {
    // Hex needs no quotes. A nonce the artifact wrote itself comes later in the tag; the first wins.
    body: html.replace(/<script\b/gi, `<script nonce=${nonce}`),
    csp: `${ARTIFACT_CSP}; script-src 'nonce-${nonce}'; script-src-attr 'unsafe-inline'`,
  };
}

/** A fresh nonce for one artifact response: 128 bits, hex. */
export function artifactNonce(): string {
  return randomBytes(16).toString("hex");
}

/** How many artifacts main holds at once for one window; that window's oldest goes first. */
export const ARTIFACTS_MAX = 16;
/** One artifact; the server keeps files up to 25 MB, the authoring guide asks for under 5 MB. */
export const ARTIFACT_BYTES_MAX = 25 * 1024 * 1024;
/** All artifacts together. */
export const ARTIFACTS_TOTAL_BYTES_MAX = 64 * 1024 * 1024;

const TOKEN = /^[0-9a-f]{32}$/;

/** The token of an artifact URL, or null for anything else. */
export function artifactToken(url: string): string | null {
  if (!url.startsWith(ARTIFACT_PREFIX)) return null;
  const rest = url
    .slice(ARTIFACT_PREFIX.length)
    .replace(/[?#].*$/, "")
    .replace(/\/$/, "");
  return TOKEN.test(rest) ? rest : null;
}

/** The only request the artifact session lets through: an artifact document itself. */
export function artifactRequestAllowed(url: string): boolean {
  return artifactToken(url) !== null;
}

interface Held {
  owner: number;
  html: string;
  bytes: number;
}

/** The artifacts main serves, in memory only, per window that asked for them. */
export class ArtifactStore {
  private readonly held = new Map<string, Held>();
  private total = 0;

  constructor(private readonly token: () => string = () => randomBytes(16).toString("hex")) {}

  /**
   * Keeps the HTML and gives the URL to show it. To make room it lets go of this window's own
   * oldest artifacts, never another window's. Throws when it is too large, or when the other
   * windows hold so much that it does not fit.
   */
  serve(owner: number, html: string): string {
    const bytes = Buffer.byteLength(html, "utf8");
    if (bytes > ARTIFACT_BYTES_MAX) throw new Error("This artifact is larger than 25 MB.");
    // A Map keeps insertion order: the first of a window's keys is its oldest.
    const own = () => [...this.held].filter(([, entry]) => entry.owner === owner).map(([token]) => token);
    let mine = own();
    while (mine.length >= ARTIFACTS_MAX || (this.total + bytes > ARTIFACTS_TOTAL_BYTES_MAX && mine.length > 0)) {
      const [oldest, ...rest] = mine;
      if (oldest !== undefined) this.drop(oldest);
      mine = rest;
    }
    if (this.total + bytes > ARTIFACTS_TOTAL_BYTES_MAX) {
      throw new Error("Other windows hold too many large artifacts; close one of them first.");
    }
    const token = this.token();
    this.held.set(token, { owner, html, bytes });
    this.total += bytes;
    return `${ARTIFACT_PREFIX}${token}`;
  }

  /** The HTML behind an artifact URL, or null when it is not (or no longer) held. */
  html(url: string): string | null {
    const token = artifactToken(url);
    return token === null ? null : (this.held.get(token)?.html ?? null);
  }

  /** Whether a URL is an artifact main holds now. */
  has(url: string): boolean {
    return this.html(url) !== null;
  }

  /** Whether a URL is an artifact main holds now for this window. */
  owns(owner: number, url: string): boolean {
    const token = artifactToken(url);
    return token !== null && this.held.get(token)?.owner === owner;
  }

  /** Lets go of one artifact, only for the window that asked for it. */
  release(owner: number, url: string): void {
    const token = artifactToken(url);
    if (token !== null && this.held.get(token)?.owner === owner) this.drop(token);
  }

  /** Lets go of everything a window asked for: it closed or reloaded. */
  releaseAll(owner: number): void {
    for (const [token, entry] of [...this.held]) if (entry.owner === owner) this.drop(token);
  }

  get size(): number {
    return this.held.size;
  }

  private drop(token: string): void {
    const entry = this.held.get(token);
    if (entry === undefined) return;
    this.held.delete(token);
    this.total -= entry.bytes;
  }
}

/**
 * Whether a WebContents that shows `own` may go to `target`, in any frame: only to its own document
 * (an anchor in it, or a reload). Anything else, also another artifact, is refused.
 */
export function artifactMayNavigate(own: string, target: string): boolean {
  const withoutHash = (url: string) => url.replace(/#.*$/, "");
  return artifactToken(own) !== null && withoutHash(own) === withoutHash(target);
}

/** Where the viewer lies over the window, in the window's content coordinates (device-independent). */
export interface ViewBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The bounds the page reported for the viewer's stage, in its CSS pixels, as window coordinates:
 * scaled by the page's zoom and kept inside the window. Null: nothing to show (hidden, off the
 * window, or not a rectangle at all).
 */
export function viewBounds(
  reported: unknown,
  zoom: number,
  content: { width: number; height: number },
): ViewBounds | null {
  if (typeof reported !== "object" || reported === null) return null;
  const r = reported as Record<string, unknown>;
  const values = [r.x, r.y, r.width, r.height];
  if (!values.every((v): v is number => typeof v === "number" && Number.isFinite(v))) return null;
  const scale = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
  const [x, y, width, height] = values.map((v) => v * scale) as [number, number, number, number];
  const left = Math.max(0, Math.round(x));
  const top = Math.max(0, Math.round(y));
  const right = Math.min(content.width, Math.round(x + width));
  const bottom = Math.min(content.height, Math.round(y + height));
  if (right <= left || bottom <= top) return null;
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** The export formats main makes itself; HTML is saved by the page. */
export type ArtifactExport = "pdf" | "png";

/** The widths the viewer shows an artifact at, and the widths an export is made at. */
export const EXPORT_WIDTH_MIN = 320;
export const EXPORT_WIDTH_MAX = 1920;
/** The height an export window starts at, and the PNG's when the artifact reports none. */
export const EXPORT_HEIGHT_DEFAULT = 900;
/** The tallest PNG: Chromium cannot paint a surface much larger. */
export const EXPORT_HEIGHT_MAX = 16_384;
/**
 * The most device pixels a PNG may have (64 Mpx, 256 MB as a bitmap). A HiDPI display multiplies
 * both sides by its scale factor, so it counts twice.
 */
export const EXPORT_PIXELS_MAX = 64_000_000;
/**
 * How long an export may take, load included. A busy artifact (an endless loop, a getter that
 * spins) never finishes by itself; after this the export fails and its renderer is ended.
 */
export const EXPORT_TIMEOUT_MS = 20_000;
/** What the page shows when an export ran out of time. */
export const EXPORT_TIMEOUT_MESSAGE = "This artifact did not finish rendering.";
/**
 * The isolated world main reads the page height in: the artifact's own prototypes (a getter on
 * `scrollHeight`) are not that world's.
 */
export const EXPORT_WORLD_ID = 1_079;

/**
 * `work`, or a failure with `message` once `ms` passed. A late result or failure of `work` is
 * dropped, so it never surfaces as an unhandled rejection.
 */
export async function withinTime<R>(work: Promise<R>, ms: number, message: string): Promise<R> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  work.catch(() => undefined);
  const limit = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error(message)), ms);
  });
  try {
    return await Promise.race([work, limit]);
  } finally {
    clearTimeout(timer);
  }
}

/** A file name for an export: the artifact's name without folders or its extension, plus the format's. */
export function exportName(name: string, kind: ArtifactExport): string {
  const base = (name.replaceAll("\\", "/").split("/").pop() ?? "").replace(/\.html?$/i, "").trim();
  return `${base || "artifact"}.${kind}`;
}

/** A width the page asked for, kept within what an export can be. */
export function exportWidth(width: unknown): number {
  if (typeof width !== "number" || !Number.isFinite(width)) return 1280;
  return Math.min(EXPORT_WIDTH_MAX, Math.max(EXPORT_WIDTH_MIN, Math.round(width)));
}

/**
 * The page height the artifact reported, kept within what can be captured: at most
 * EXPORT_HEIGHT_MAX, and lower when `width` at `scale` would pass EXPORT_PIXELS_MAX.
 */
export function captureHeight(reported: unknown, fallback: number, width: number, scale = 1): number {
  const factor = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const tallest = Math.min(EXPORT_HEIGHT_MAX, Math.floor(EXPORT_PIXELS_MAX / (width * factor * factor)));
  if (typeof reported !== "number" || !Number.isFinite(reported) || reported < 1) return Math.min(fallback, tallest);
  return Math.min(tallest, Math.ceil(reported));
}
