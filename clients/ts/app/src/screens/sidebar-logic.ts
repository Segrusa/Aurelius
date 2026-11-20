// The sidebar's lists without a screen: a Sessions and a Tickets section, each cut into Running, Today
// and Earlier, per workspace when All workspaces shows, and what each workspace's rail tile counts.

import type { SessionSignal } from "./session-logic";

/** The sidebar's two sections; each folds on its own and always shows. */
export type SidebarSection = "sessions" | "tickets";

/** One row of the sidebar: a work session or a ticket's session. */
export interface SidebarItem {
  /** Unique across both kinds. */
  key: string;
  section: SidebarSection;
  to: string;
  label: string;
  /** Who works in it: the session's employee or the ticket's owner, for the mark. */
  who: string;
  whoId: string;
  projectId: string | null;
  signal: SessionSignal | null;
  /** Its own last activity: opening it must not move it. */
  at: number;
}

export type Bucket = "running" | "today" | "earlier";

export const BUCKETS: readonly Bucket[] = ["running", "today", "earlier"];

export const BUCKET_LABELS: Record<Bucket, string> = { running: "Running", today: "Today", earlier: "Earlier" };

/** How many rows Earlier lists per workspace: the rest is in History. */
export const EARLIER_MAX = 20;

/**
 * Where a row goes: Running while it works; Today when it was active from `since` on, or waits for you, so a
 * question is never folded away under Earlier; Earlier otherwise.
 */
export function bucketOf(item: Pick<SidebarItem, "signal" | "at">, since: number): Bucket {
  if (item.signal === "working") return "running";
  if (item.signal === "needs_you" || item.at >= since) return "today";
  return "earlier";
}

export interface SidebarBucket {
  bucket: Bucket;
  items: SidebarItem[];
}

/** The non-empty buckets of these rows, newest first in each; Earlier holds at most `EARLIER_MAX`. */
export function bucketed(items: SidebarItem[], since: number): SidebarBucket[] {
  const sorted = [...items].sort((a, b) => b.at - a.at);
  return BUCKETS.map((bucket) => {
    const mine = sorted.filter((i) => bucketOf(i, since) === bucket);
    return { bucket, items: bucket === "earlier" ? mine.slice(0, EARLIER_MAX) : mine };
  }).filter((b) => b.items.length > 0);
}

export interface WorkspaceGroup {
  /** A project id; null for what has no project. */
  id: string | null;
  buckets: SidebarBucket[];
  /** Rows listed under it. */
  count: number;
  running: number;
}

/**
 * All workspaces: a group per project in the rail's order, then one for what has no project. A project
 * without rows gets no group.
 */
export function workspaceGroups(items: SidebarItem[], projectIds: string[], since: number): WorkspaceGroup[] {
  const known = new Set(projectIds);
  const ids: (string | null)[] = [...projectIds, null];
  return ids
    .map((id) => {
      const mine = items.filter((i) =>
        id === null ? i.projectId === null || !known.has(i.projectId) : i.projectId === id,
      );
      const buckets = bucketed(mine, since);
      const count = buckets.reduce((n, b) => n + b.items.length, 0);
      const running = buckets.find((b) => b.bucket === "running")?.items.length ?? 0;
      return { id, buckets, count, running };
    })
    .filter((g) => g.count > 0);
}

/** How many rows wait for you per project: the red count on its rail tile. */
export function needsPerProject(items: Pick<SidebarItem, "projectId" | "signal">[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const i of items) {
    if (i.signal !== "needs_you" || i.projectId === null) continue;
    counts.set(i.projectId, (counts.get(i.projectId) ?? 0) + 1);
  }
  return counts;
}

/** The storage key of a sidebar section's own fold. */
export function sectionKey(section: SidebarSection): string {
  return `sidebar.${section}`;
}

/**
 * The storage key of a fold inside a section; `workspace` only under All workspaces. The Sessions section
 * keeps the keys it had before Tickets became a section of its own, so folds made then still hold.
 */
export function foldKey(
  workspace: string | null | undefined,
  bucket?: Bucket,
  section: SidebarSection = "sessions",
): string {
  const ws = workspace === undefined ? "" : (workspace ?? "none");
  const prefix = section === "sessions" ? "sidebar.fold" : "sidebar.tickets.fold";
  return [prefix, ws, bucket ?? ""].filter((p) => p !== "").join(".");
}

/** Whether a fold is closed: as stored, else Earlier starts closed and the rest open. */
export function isFolded(stored: string | null, bucket?: Bucket): boolean {
  if (stored === "folded") return true;
  if (stored === "open") return false;
  return bucket === "earlier";
}
