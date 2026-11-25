// The review-first Code tab (ticket #84, design/code-review.html): which branches wait for the board member's
// review, which are still being worked on, what can be tidied up, and the checks of one branch. Pure functions
// over what `code.get`, `ticket.list` and `work_session.list` send, so they are tested without a repository.

import type { CodeBranch, CodeView, CodeWorktree, TicketView, WorkSessionView } from "@aurelius/protocol";
import { branchStatus } from "./code-logic";

export type ReviewKind = "ticket" | "session";
export type QueueFilter = "all" | ReviewKind;

export const QUEUE_FILTERS: { value: QueueFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "ticket", label: "Tickets" },
  { value: "session", label: "Sessions" },
];

/** A branch that belongs to a ticket or a work session, with what the queue and the review show of it. */
export interface ReviewItem {
  /** The branch name: one branch is one review. */
  key: string;
  kind: ReviewKind;
  branch: CodeBranch;
  title: string;
  /** The ticket's number; none for a session. */
  number: number | null;
  ticket: TicketView | null;
  session: WorkSessionView | null;
  ownerId: string | null;
  ownerName: string | null;
  /** Since when it waits: the ticket's last change (it went to Review), or the session's last activity. */
  since: number;
  /** Files changed in its worktree and not committed yet. */
  uncommitted: number;
  /** Its worktree's folder; for a session on its own branch that is the project's folder. */
  where: string | null;
}

export interface ReviewQueue {
  /** Tickets in Review and idle sessions with work main lacks: the oldest waiting first. */
  ready: ReviewItem[];
  /** Branches an employee works on now: read-only. */
  working: ReviewItem[];
}

/** The worktree a branch is checked out in, when it has one. */
function worktreeOf(view: Pick<CodeView, "worktrees">, b: CodeBranch): CodeWorktree | null {
  return (view.worktrees ?? []).find((w) => w.branch === b.name && !w.missing) ?? null;
}

function uncommittedOf(w: CodeWorktree | null): number {
  if (!w) return 0;
  return w.changed_count ?? w.changed_files?.length ?? 0;
}

/** The ticket or session a branch belongs to, as a review item; null for a branch of neither. */
export function itemOf(
  view: Pick<CodeView, "worktrees">,
  b: CodeBranch,
  tickets: ReadonlyMap<string, TicketView>,
  sessions: ReadonlyMap<string, WorkSessionView>,
): ReviewItem | null {
  if (b.is_main) return null;
  const w = worktreeOf(view, b);
  const base = { key: b.name, branch: b, uncommitted: uncommittedOf(w), where: w?.path ?? null };
  const ticket = b.ticket ? (tickets.get(b.ticket.ticket_id) ?? null) : null;
  if (b.ticket) {
    return {
      ...base,
      kind: "ticket",
      title: ticket?.title ?? b.ticket.title,
      number: b.ticket.number,
      ticket,
      session: null,
      ownerId: b.owner?.employee_id ?? ticket?.owner_employee_id ?? null,
      ownerName: b.owner?.name ?? ticket?.owner_name ?? null,
      since: ticket?.updated_at_ms ?? b.at_ms,
    };
  }
  const session = sessions.get(b.name);
  if (!session) return null;
  return {
    ...base,
    kind: "session",
    title: session.title,
    number: null,
    ticket: null,
    session,
    ownerId: session.employee_id,
    ownerName: session.employee_name,
    since: session.last_activity_ms,
  };
}

/** Whether an item waits for review: nobody works on it, and it holds work main lacks (committed or not). */
export function isReady(item: ReviewItem): boolean {
  if (item.branch.working) return false;
  if (item.branch.ahead === 0 && item.uncommitted === 0) return false;
  if (item.kind === "ticket") return item.ticket?.column === "review";
  return item.session !== null && !item.session.running;
}

/**
 * The queue: every branch of a ticket or a work session that waits for review, the oldest waiting first, and
 * beside it what employees work on now. A session without a branch of its own works in the project's folder
 * on main and is never listed.
 */
export function reviewQueue(
  view: Pick<CodeView, "worktrees" | "branches">,
  tickets: readonly TicketView[],
  sessions: readonly WorkSessionView[],
): ReviewQueue {
  const byTicket = new Map(tickets.map((t) => [t.id, t]));
  const byBranch = new Map(sessions.filter((s) => s.branch).map((s) => [s.branch as string, s]));
  const ready: ReviewItem[] = [];
  const working: ReviewItem[] = [];
  for (const b of view.branches ?? []) {
    const item = itemOf(view, b, byTicket, byBranch);
    if (!item) continue;
    if (b.working) working.push(item);
    else if (isReady(item)) ready.push(item);
  }
  ready.sort((a, b) => a.since - b.since || a.key.localeCompare(b.key));
  working.sort((a, b) => b.since - a.since || a.key.localeCompare(b.key));
  return { ready, working };
}

export function filterQueue(items: readonly ReviewItem[], filter: QueueFilter): ReviewItem[] {
  return filter === "all" ? [...items] : items.filter((i) => i.kind === filter);
}

/** The item to show: the one asked for while it is in the list, else the first. */
export function currentItem(items: readonly ReviewItem[], wanted: string | null): ReviewItem | null {
  return items.find((i) => i.key === wanted) ?? items[0] ?? null;
}

/** What Tidy up offers: merged branches nobody needs any more and worktrees left behind. */
export interface Tidy {
  branches: string[];
  worktrees: string[];
}

export function tidyUp(view: Pick<CodeView, "worktrees" | "branches">, now: number): Tidy {
  const gone = (view.branches ?? []).filter((b) => !b.is_main && !b.open && branchStatus(b, now) === "merged");
  const goneNames = new Set(gone.map((b) => b.name));
  const worktrees = (view.worktrees ?? []).filter(
    (w) =>
      !w.is_main &&
      !w.working &&
      (w.missing || (w.branch !== null && w.branch !== undefined && goneNames.has(w.branch) && uncommittedOf(w) === 0)),
  );
  return { branches: gone.map((b) => b.name), worktrees: worktrees.map((w) => w.path) };
}

/** "6 branches are merged into main, 2 worktrees are left behind." in parts, or null when nothing is left. */
export function tidyText(t: Tidy, main: string): string | null {
  const n = t.branches.length;
  const w = t.worktrees.length;
  if (n === 0 && w === 0) return null;
  const parts: string[] = [];
  if (n > 0) parts.push(`${n} ${n === 1 ? "branch is" : "branches are"} merged into ${main}`);
  if (w > 0) parts.push(`${w} ${w === 1 ? "worktree is" : "worktrees are"} left behind`);
  return `${parts.join(", ")}.`;
}

/** "25 min", "2 h", "1 d": how long something waits, in the queue's chips. */
export function waited(since: number, now: number): string {
  const m = Math.max(0, Math.floor((now - since) / 60_000));
  if (m < 1) return "now";
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h`;
  return `${Math.floor(h / 24)} d`;
}

export type CheckTone = "ok" | "warn" | "bad" | "n";

/** One of the cards above the review: what it checks, its verdict and a line under it. */
export interface Check {
  name: string;
  tone: CheckTone;
  value: string;
  sub: string;
}

/** The board member's own verdict in this window, before the server keeps reviews (follow-up). */
export type Verdict = "comment" | "approved" | "changes";

export function reviewCheck(item: ReviewItem, mine: Verdict | null): Check {
  if (mine === "approved") return { name: "Review", tone: "ok", value: "Approved", sub: "by you" };
  if (mine === "changes") return { name: "Review", tone: "warn", value: "Changes requested", sub: "by you" };
  if (item.kind === "session")
    return { name: "Review", tone: "n", value: "Not reviewed", sub: "a session has no review step" };
  return { name: "Review", tone: "n", value: "Waits for you", sub: `ticket #${item.number} is in Review` };
}

/** Whether the branch can merge, as far as git's numbers tell before a conflict check (follow-up). */
export function mergeCheck(item: ReviewItem, main: string): Check {
  const { behind } = item.branch;
  if (item.uncommitted > 0)
    return {
      name: "Merge",
      tone: "warn",
      value: "Not committed",
      sub: `${item.uncommitted} ${item.uncommitted === 1 ? "file" : "files"} changed since the last commit`,
    };
  if (item.branch.ahead === 0)
    return { name: "Merge", tone: "n", value: "Nothing to merge", sub: `no commits ahead of ${main}` };
  if (behind === 0) return { name: "Merge", tone: "ok", value: "Up to date", sub: `up to date with ${main}` };
  return {
    name: "Merge",
    tone: "warn",
    value: `${behind} behind`,
    sub: `${main} moved on by ${behind} ${behind === 1 ? "commit" : "commits"}; git merges or stops on a conflict`,
  };
}

export function sizeCheck(
  b: Pick<CodeBranch, "files_changed" | "insertions" | "deletions" | "ahead">,
  uncommitted: number,
): Check {
  const files = b.files_changed ?? 0;
  const commits = `${b.ahead} ${b.ahead === 1 ? "commit" : "commits"}`;
  return {
    name: "Size",
    tone: "n",
    value: `${files} ${files === 1 ? "file" : "files"}`,
    sub: `+${b.insertions ?? 0} −${b.deletions ?? 0} · ${commits}${uncommitted > 0 ? " + changes" : ""}`,
  };
}

/** Why Merge into main is off, or null when it is on. */
export function mergeBlocked(item: ReviewItem): string | null {
  if (item.branch.working) return `${item.ownerName ?? "Someone"} is working on it.`;
  if (item.uncommitted > 0) return `${item.ownerName ?? "The owner"} has to commit first.`;
  if (item.branch.ahead === 0) return "Nothing to merge.";
  return null;
}

/** The line under the title: "3 behind main" or "up to date with main". */
export function behindText(b: Pick<CodeBranch, "behind">, main: string): string {
  return b.behind > 0 ? `${b.behind} behind ${main}` : `up to date with ${main}`;
}

/** What a submitted review posts, in the words the owner reads in the discussion or the session. */
export function reviewPost(verdict: Verdict, body: string, branch: string): string {
  const text = body.trim();
  if (verdict === "approved") return text ? `Approved ${branch}.\n\n${text}` : `Approved ${branch}.`;
  if (verdict === "changes")
    return text ? `Changes requested on ${branch}:\n\n${text}` : `Changes requested on ${branch}.`;
  return text;
}

/** Whether Submit review can go: a comment and a request for changes need words, an approval does not. */
export function canSubmit(verdict: Verdict, body: string): boolean {
  return verdict === "approved" || body.trim() !== "";
}

/** The message Ask … to commit proposes. */
export function commitAsk(branch: string): string {
  return `Please commit your changes with their tests, so I can merge ${branch}.`;
}

/** The message Ask Trace to review sends to the reviewer. */
export function traceAsk(item: ReviewItem, repo: string, main: string): string {
  const what = item.kind === "ticket" ? `ticket #${item.number} (${item.title})` : `the work session "${item.title}"`;
  return `Please review branch ${item.branch.name} of ${what} in ${repo} against ${main}, and post your findings with their file and line.`;
}

/** The head's line about main: "main · 4 ahead of origin · fetched 12 min ago". */
export function mainlineText(
  view: Pick<CodeView, "branches" | "main_branch" | "has_remote" | "fetched_at_ms">,
  now: number,
): string {
  const main = view.main_branch ?? "main";
  const b = (view.branches ?? []).find((x) => x.is_main);
  const parts = [main];
  if (b?.upstream) {
    const remote = b.upstream.split("/")[0] ?? b.upstream;
    const ahead = b.upstream_ahead ?? 0;
    const behind = b.upstream_behind ?? 0;
    if (ahead === 0 && behind === 0) parts.push(`up to date with ${remote}`);
    if (ahead > 0) parts.push(`${ahead} ahead of ${remote}`);
    if (behind > 0) parts.push(`${behind} behind ${remote}`);
  }
  if (!view.has_remote) parts.push("no remote");
  else if (view.fetched_at_ms === null || view.fetched_at_ms === undefined) parts.push("never fetched");
  else
    parts.push(
      `fetched ${waited(view.fetched_at_ms, now) === "now" ? "just now" : `${waited(view.fetched_at_ms, now)} ago`}`,
    );
  return parts.join(" · ");
}
