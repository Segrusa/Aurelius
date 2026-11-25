// The review-first Code tab (ticket #84, design/code-review.html): which branches wait for the board member's
// review, which are still being worked on, what can be tidied up, and the checks of one branch. Pure functions
// over what `code.get`, `ticket.list` and `work_session.list` send, so they are tested without a repository.

import type {
  CodeBranch,
  CodeFileStatus,
  CodeHunk,
  CodeLineSide,
  CodeReviewEntry,
  CodeReviewView,
  CodeThreadView,
  CodeView,
  CodeWorktree,
  EmployeeSummary,
  MessageView,
  TicketView,
  WorkSessionView,
} from "@aurelius/protocol";
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

/** The verdict the Review check shows: who gave it, and whether it was the reader. */
export interface VerdictOf {
  verdict: "approved" | "changes";
  by: string;
  mine: boolean;
}

/**
 * The verdict at this head: the reader's newest approval or request for changes, else another board member's.
 * A verdict on an older head is not one on this head; a review with only comments is no verdict.
 */
export function verdictAt(reviews: readonly CodeReviewEntry[], head: string): VerdictOf | null {
  const at = reviews.filter(
    (r): r is CodeReviewEntry & { verdict: "approved" | "changes" } =>
      r.head === head.toLowerCase() && (r.verdict === "approved" || r.verdict === "changes"),
  );
  const newest = (list: typeof at) =>
    list.reduce<(typeof at)[number] | null>((a, r) => (!a || r.created_at_ms >= a.created_at_ms ? r : a), null);
  const pick = newest(at.filter((r) => r.mine)) ?? newest(at);
  return pick ? { verdict: pick.verdict, by: pick.username, mine: pick.mine ?? false } : null;
}

export function reviewCheck(item: ReviewItem, v: VerdictOf | null): Check {
  const by = v?.mine ? "by you" : `by ${v?.by}`;
  if (v?.verdict === "approved") return { name: "Review", tone: "ok", value: "Approved", sub: by };
  if (v?.verdict === "changes") return { name: "Review", tone: "warn", value: "Changes requested", sub: by };
  if (item.kind === "session")
    return { name: "Review", tone: "n", value: "Not reviewed", sub: "a session has no review step" };
  return { name: "Review", tone: "n", value: "Waits for you", sub: `ticket #${item.number} is in Review` };
}

/** Whether the branch can merge: git's conflict check when the server has one, else how far main moved on. */
export function mergeCheck(item: ReviewItem, main: string): Check {
  const { behind, conflicts } = item.branch;
  const moved = `${main} moved on by ${behind} ${behind === 1 ? "commit" : "commits"}`;
  if (item.uncommitted > 0)
    return {
      name: "Merge",
      tone: "warn",
      value: "Not committed",
      sub: `${item.uncommitted} ${item.uncommitted === 1 ? "file" : "files"} changed since the last commit`,
    };
  if (item.branch.ahead === 0)
    return { name: "Merge", tone: "n", value: "Nothing to merge", sub: `no commits ahead of ${main}` };
  if (conflicts && conflicts.length > 0)
    return { name: "Merge", tone: "bad", value: "Conflict", sub: `${conflicts.join(", ")}, also changed on ${main}` };
  if (conflicts)
    return { name: "Merge", tone: "ok", value: "No conflicts", sub: behind > 0 ? moved : `up to date with ${main}` };
  if (behind === 0) return { name: "Merge", tone: "ok", value: "Up to date", sub: `up to date with ${main}` };
  return { name: "Merge", tone: "warn", value: `${behind} behind`, sub: `${moved}; git merges or stops on a conflict` };
}

/** The system line the Testing column writes when its tester gives a verdict. */
const TESTED = { passed: "tested_passed", failed: "tested_failed" } as const;

/** The tester's newest verdict in a ticket's discussion; a session has no Testing step. */
export function testsCheck(item: ReviewItem, discussion: readonly MessageView[] | undefined, now: number): Check {
  if (item.kind === "session")
    return { name: "Tests", tone: "n", value: "Not run", sub: "a session has no Testing step" };
  const line = [...(discussion ?? [])]
    .reverse()
    .find((m) => m.system?.action === TESTED.passed || m.system?.action === TESTED.failed);
  if (!line) return { name: "Tests", tone: "n", value: "Not run", sub: "no Testing step ran for this ticket" };
  const passed = line.system?.action === TESTED.passed;
  // "Tess: the tests passed · 717 passed, 0 failed": the summary after the dot.
  const summary = line.text.split(" · ").slice(1).join(" · ").trim();
  const when = waited(line.created_at_ms, now);
  return {
    name: "Tests",
    tone: passed ? "ok" : "bad",
    value: passed ? "Passed" : "Failed",
    sub: `${summary ? `${summary} · ` : ""}${when === "now" ? "just now" : `${when} ago`}`,
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
  if ((item.branch.conflicts ?? []).length > 0)
    return `${item.ownerName ?? "The owner"} has to resolve the conflict first.`;
  if (item.uncommitted > 0) return `${item.ownerName ?? "The owner"} has to commit first.`;
  if (item.branch.ahead === 0) return "Nothing to merge.";
  return null;
}

/** The line under the title: "3 behind main" or "up to date with main". */
export function behindText(b: Pick<CodeBranch, "behind">, main: string): string {
  return b.behind > 0 ? `${b.behind} behind ${main}` : `up to date with ${main}`;
}

/** A line comment as a review sends it. */
export interface LineNote {
  path: string;
  line: number;
  body: string;
}

/**
 * What a submitted review posts, in the words the owner reads in the discussion or the session: the verdict, the
 * summary, then each line comment with its file and line.
 */
export function reviewPost(verdict: Verdict, body: string, branch: string, notes: readonly LineNote[] = []): string {
  const text = body.trim();
  const lines = notes.map((n) => `- \`${n.path}:${n.line}\` ${n.body.trim()}`).join("\n");
  const tail = lines ? `${text ? "\n\n" : ""}Line comments:\n${lines}` : "";
  const words = `${text}${tail}`;
  if (verdict === "approved") return words ? `Approved ${branch}.\n\n${words}` : `Approved ${branch}.`;
  if (verdict === "changes")
    return words ? `Changes requested on ${branch}:\n\n${words}` : `Changes requested on ${branch}.`;
  return words;
}

/** Whether Submit review can go: an approval always, a comment or a request for changes with words or a line comment. */
export function canSubmit(verdict: Verdict, body: string, pending = 0): boolean {
  return verdict === "approved" || body.trim() !== "" || pending > 0;
}

/** The reader's comments that wait for Finish review, with where they are. */
export function pendingNotes(review: Pick<CodeReviewView, "threads"> | undefined): LineNote[] {
  return (review?.threads ?? []).flatMap((t) =>
    (t.comments ?? []).filter((c) => c.pending && c.mine).map((c) => ({ path: t.path, line: t.line, body: c.body })),
  );
}

/** One row of a file's diff, with its numbers on each side and where a comment on it goes. */
export interface DiffRow {
  kind: "ctx" | "add" | "del";
  old: number | null;
  new: number | null;
  /** The line as git wrote it, its ' ', '+' or '-' first. */
  raw: string;
  side: CodeLineSide;
  line: number;
}

/** The rows of a hunk: a removed line is on main's side, any other on the branch's. */
export function diffRows(hunk: Pick<CodeHunk, "old_start" | "new_start" | "lines">): DiffRow[] {
  let o = hunk.old_start;
  let n = hunk.new_start;
  return (hunk.lines ?? []).map((raw) => {
    if (raw.startsWith("+")) return { kind: "add", old: null, new: n, raw, side: "new", line: n++ } as DiffRow;
    if (raw.startsWith("-")) return { kind: "del", old: o, new: null, raw, side: "old", line: o++ } as DiffRow;
    const row: DiffRow = { kind: "ctx", old: o, new: n, raw, side: "new", line: n };
    o++;
    n++;
    return row;
  });
}

/** The lines a new thread keeps of the code it is on: up to three, the commented one last. */
export function snippetAt(rows: readonly DiffRow[], index: number): string[] {
  return rows.slice(Math.max(0, index - 2), index + 1).map((r) => r.raw);
}

/** The threads on one line of a file. */
export function threadsOn(
  threads: readonly CodeThreadView[],
  path: string,
  side: CodeLineSide,
  line: number,
): CodeThreadView[] {
  return threads.filter((t) => t.path === path && t.side === side && t.line === line);
}

/** A thread made on an older head: its line may have moved since. */
export function outdated(thread: Pick<CodeThreadView, "commit">, head: string): boolean {
  return thread.commit.toLowerCase() !== head.toLowerCase();
}

/** "screens/sidebar.tsx" for a long path: the last two parts, as the file tree shows them. */
export function fileLabel(path: string): string {
  return path.split("/").slice(-2).join("/");
}

/** The letter of a file's change, as in the design. */
export const STATUS_LETTER: Record<CodeFileStatus, "A" | "M" | "D" | "R"> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
};

/** Open threads per file, for the tree's bubble. */
export function openThreadsIn(threads: readonly CodeThreadView[], path: string): number {
  return threads.filter((t) => t.path === path && !t.resolved).length;
}

/** The message Ask … to commit proposes. */
export function commitAsk(branch: string): string {
  return `Please commit your changes with their tests, so I can merge ${branch}.`;
}

/**
 * The message Ask Trace to review sends to the reviewer. It names the folder the branch is checked out in, so
 * uncommitted work is reviewed too, and asks to leave that folder as it is: in the project folder another session
 * may be working.
 */
export function traceAsk(item: ReviewItem, repo: string, main: string): string {
  const what = item.kind === "ticket" ? `ticket #${item.number} (${item.title})` : `the work session "${item.title}"`;
  const where = item.where ?? repo;
  const changes =
    item.uncommitted > 0
      ? ` It also has ${item.uncommitted} uncommitted ${item.uncommitted === 1 ? "file" : "files"} there: review those too.`
      : "";
  return (
    `Please review branch ${item.branch.name} of ${what} against ${main}. It is checked out in ${where}.${changes} ` +
    "Only read: do not switch branches, commit or change files in that folder. " +
    "Post your findings with their file and line."
  );
}

/** Where Ask Trace to review stands for one branch. */
export interface TraceAskState {
  /** The ask is on its way to the server. */
  sending: boolean;
  /** Trace's chat has a run going. */
  working: boolean;
  /** This window asked Trace to review this branch. */
  asked: boolean;
}

/** Whether the ask button is offered; otherwise the page offers to open Trace's chat. */
export function canAskTrace(s: TraceAskState): boolean {
  return !s.sending && !s.working;
}

/** The line beside the ask in the Conversation tab. */
export function traceAskLine(s: TraceAskState): string {
  if (s.sending) return "Asking Trace to review…";
  if (s.working && s.asked) return "Trace is reviewing this branch. The findings come in Trace's chat.";
  if (s.working) return "Trace is busy in their chat. Ask again when that answer is done.";
  if (s.asked) return "Trace answered in their chat. Open it for the findings, or ask again.";
  return "No review yet. A session has no review step of its own.";
}

/** Whether the reviewer's own chat has a run going: a new ask then waits until it is done. */
export function chatRunning(e: Pick<EmployeeSummary, "agents" | "conversation_id">): boolean {
  return e.agents.some((a) => a.kind === "conversation" && a.conversation_id === e.conversation_id);
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
