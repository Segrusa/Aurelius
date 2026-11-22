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
 * The policy of every artifact response. Inline code runs; nothing can be fetched or sent. `sandbox`
 * makes it a sandbox of its own also where it is not in the viewer's frame (the export window):
 * an opaque origin without storage, popups, forms or navigation of the top.
 */
export const ARTIFACT_CSP =
  "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'";

/** How many artifacts main holds at once, over all windows; the oldest goes first. */
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

  /** Keeps the HTML and gives the URL to frame it. Throws when it is too large. */
  serve(owner: number, html: string): string {
    const bytes = Buffer.byteLength(html, "utf8");
    if (bytes > ARTIFACT_BYTES_MAX) throw new Error("This artifact is larger than 25 MB.");
    // A Map keeps insertion order: the first key is the oldest.
    while (this.held.size >= ARTIFACTS_MAX || this.total + bytes > ARTIFACTS_TOTAL_BYTES_MAX) {
      const oldest = this.held.keys().next();
      if (oldest.done) break;
      this.drop(oldest.value);
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

/** The page height the artifact reported, kept within what can be captured. */
export function captureHeight(reported: unknown, fallback: number): number {
  if (typeof reported !== "number" || !Number.isFinite(reported) || reported < 1) return fallback;
  return Math.min(EXPORT_HEIGHT_MAX, Math.ceil(reported));
}
