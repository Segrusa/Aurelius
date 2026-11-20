// When a page's link to the server must close, and a short record of every link that did. Kept apart
// from Electron so the rules can be tested.

import { appendFileSync, mkdirSync, renameSync, statSync } from "node:fs";
import { dirname } from "node:path";

/** What the navigation events of a window say about its page. */
export interface NavigationDetails {
  isMainFrame: boolean;
  isSameDocument: boolean;
}

/**
 * Follows a window's page changes for one link. Electron sends `did-start-navigation` before
 * `will-navigate`, so also for a change the app blocks (a dropped file, a plain link); only a change
 * that commits (`did-navigate`) replaces the page. The page that opened the link committed before
 * it asked, so a commit without a start since is that page's own.
 */
export class PageWatch {
  private leaving = false;

  /** `did-start-navigation`. */
  started(details: NavigationDetails): void {
    if (details.isMainFrame && !details.isSameDocument) this.leaving = true;
  }

  /** `did-navigate`: true when the page that owns the link is gone now. */
  committed(): boolean {
    return this.leaving;
  }
}

/** The log may grow to this before it moves to `.1` and starts again. */
export const LINK_LOG_MAX_BYTES = 1024 * 1024;

/** One line about a link; never what went over it. */
export function linkLine(
  at: Date,
  id: number,
  event: "opened" | "closed",
  detail: Record<string, string | number>,
): string {
  const fields = Object.entries(detail).map(([key, value]) => `${key}=${JSON.stringify(value)}`);
  return [at.toISOString(), `link=${id}`, event, ...fields].join(" ");
}

/** Appends lines to `file`, keeping at most the current file and one older one. Never throws. */
export function appendLinkLog(file: string, line: string, maxBytes = LINK_LOG_MAX_BYTES): void {
  try {
    mkdirSync(dirname(file), { recursive: true });
    try {
      if (statSync(file).size >= maxBytes) renameSync(file, `${file}.1`);
    } catch {
      // No log yet.
    }
    appendFileSync(file, `${line}\n`, { mode: 0o600 });
  } catch {
    // A log is never worth failing a connection over.
  }
}
