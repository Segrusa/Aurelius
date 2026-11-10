// What the Changes screen shows without a screen: a unified diff as files and lines, what a
// change or a branch says in words, and what of the screen is in the URL.

import type { GitBranch, GitChange } from "@aurelius/protocol";

/** The server reads git (feature `project_git`); without it there is no Changes screen. */
export const GIT_FEATURE = "project_git";

/** A branch and a worktree per ticket (feature `ticket_worktrees`). */
export const WORKTREES_FEATURE = "ticket_worktrees";

/** A board member merges or pushes a ticket's branch (feature `ticket_landing`). */
export const LANDING_FEATURE = "ticket_landing";

/** What the review bar says of a ticket's branch before the decision on it. */
export function branchLine(t: { branch?: string | null; base_branch?: string | null; landed?: string | null }): string {
  if (t.landed === "merged") return `${t.branch} is merged into ${t.base_branch}.`;
  if (t.landed === "pushed") return `${t.branch} is pushed for a pull request into ${t.base_branch}.`;
  return `The work is on ${t.branch}, not in ${t.base_branch} yet:`;
}

export const CHANGES_VIEWS = ["changes", "commits", "branches"] as const;
export type ChangesView = (typeof CHANGES_VIEWS)[number];

/** What is chosen, kept in the URL so a link opens the same commit or file. */
export interface ChangesSearch {
  view?: ChangesView;
  /** The branch whose commits are listed; none: where the work folder is. */
  rev?: string;
  /** Only what `rev` has that this branch has not: what a ticket's branch did. */
  base?: string;
  commit?: string;
  path?: string;
}

const text = (value: unknown): string | undefined =>
  typeof value === "string" && value.length > 0 && value.length <= 4000 ? value : undefined;

export function changesSearch(search: Record<string, unknown>): ChangesSearch {
  const view = CHANGES_VIEWS.find((v) => v === search.view);
  const out: ChangesSearch = {};
  if (view && view !== "changes") out.view = view;
  const rev = text(search.rev);
  if (rev) out.rev = rev;
  const base = text(search.base);
  if (rev && base) out.base = base;
  const commit = text(search.commit);
  if (commit && /^[0-9a-f]{4,64}$/i.test(commit)) out.commit = commit;
  const path = text(search.path);
  if (path) out.path = path;
  return out;
}

export type DiffLineKind = "add" | "del" | "ctx" | "hunk" | "note";

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
  /** Line numbers in the old and the new file, where the line has one. */
  before?: number;
  after?: number;
}

export interface DiffFile {
  path: string;
  /** Where it was before a rename. */
  from?: string;
  /** `added`, `deleted`, `renamed` or `modified`. */
  status: string;
  binary: boolean;
  added: number;
  removed: number;
  lines: DiffLine[];
}

const strip = (name: string): string => name.replace(/^"?[ab]\//, "").replace(/"$/, "");

/** A unified diff as git prints it, one entry per file. Lines it does not know stay out. */
export function parseDiff(diff: string): DiffFile[] {
  const files: DiffFile[] = [];
  let file: DiffFile | null = null;
  let before = 0;
  let after = 0;
  let inHunk = false;
  for (const line of diff.split("\n")) {
    if (line.startsWith("diff --git ")) {
      // "diff --git a/old b/new": the new name is the path until a line below says otherwise.
      const names = line.slice("diff --git ".length);
      const at = names.indexOf(" b/");
      file = {
        path: at >= 0 ? names.slice(at + 3) : strip(names),
        status: "modified",
        binary: false,
        added: 0,
        removed: 0,
        lines: [],
      };
      files.push(file);
      inHunk = false;
      continue;
    }
    if (!file) continue;
    if (!inHunk) {
      if (line.startsWith("new file mode")) file.status = "added";
      else if (line.startsWith("deleted file mode")) file.status = "deleted";
      else if (line.startsWith("rename from ")) {
        file.from = line.slice("rename from ".length);
        file.status = "renamed";
      } else if (line.startsWith("rename to ")) file.path = line.slice("rename to ".length);
      else if (line.startsWith("+++ ") && line !== "+++ /dev/null") file.path = strip(line.slice(4));
      else if (line.startsWith("--- ") && line !== "--- /dev/null" && file.status === "deleted")
        file.path = strip(line.slice(4));
      else if (line.startsWith("Binary files ") || line.startsWith("GIT binary patch")) file.binary = true;
    }
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (hunk) {
      before = Number(hunk[1]);
      after = Number(hunk[2]);
      inHunk = true;
      file.lines.push({ kind: "hunk", text: line });
      continue;
    }
    if (!inHunk) continue;
    if (line.startsWith("+")) {
      file.lines.push({ kind: "add", text: line.slice(1), after });
      file.added += 1;
      after += 1;
    } else if (line.startsWith("-")) {
      file.lines.push({ kind: "del", text: line.slice(1), before });
      file.removed += 1;
      before += 1;
    } else if (line.startsWith(" ")) {
      file.lines.push({ kind: "ctx", text: line.slice(1), before, after });
      before += 1;
      after += 1;
    } else if (line.startsWith("\\")) {
      file.lines.push({ kind: "note", text: line.slice(2) });
    }
  }
  return files;
}

const STATUS: Record<string, { letter: string; label: string }> = {
  added: { letter: "A", label: "Added" },
  modified: { letter: "M", label: "Modified" },
  deleted: { letter: "D", label: "Deleted" },
  renamed: { letter: "R", label: "Renamed" },
  copied: { letter: "C", label: "Copied" },
  untracked: { letter: "N", label: "New, not in git yet" },
  conflict: { letter: "!", label: "Conflict" },
};

/** A letter for the list and words for the tooltip; a kind of a newer server is "Changed". */
export function statusOf(status: string): { letter: string; label: string } {
  return STATUS[status] ?? { letter: "•", label: "Changed" };
}

export function statusTone(status: string): "add" | "del" | "warn" | "plain" {
  if (status === "added" || status === "untracked" || status === "copied") return "add";
  if (status === "deleted") return "del";
  return status === "conflict" ? "warn" : "plain";
}

/** "+12 −3", only what there is; nothing for a binary or a new file git has not counted. */
export function counts(change: { added?: number | null; removed?: number | null }): string {
  const parts: string[] = [];
  if (change.added) parts.push(`+${change.added}`);
  if (change.removed) parts.push(`−${change.removed}`);
  return parts.join(" ");
}

/** The folder and the name of a path, so a list shows the name first. */
export function splitPath(path: string): { dir: string; name: string } {
  const at = path.lastIndexOf("/");
  return at < 0 ? { dir: "", name: path } : { dir: path.slice(0, at + 1), name: path.slice(at + 1) };
}

/** Where a ticket's work is read: the commits its branch has that its base has not. */
export function ticketChanges(t: { branch?: string | null; base_branch?: string | null }): ChangesSearch | null {
  if (!t.branch) return null;
  return { view: "commits", rev: t.branch, ...(t.base_branch ? { base: t.base_branch } : {}) };
}

export const shortId = (id: string): string => id.slice(0, 8);

/** How a branch stands to what it follows. */
export function trackLine(branch: GitBranch): string {
  if (branch.remote) return "";
  if (!branch.upstream) return "Not pushed";
  if (branch.ahead == null || branch.behind == null) return `${branch.upstream} is gone`;
  if (branch.ahead === 0 && branch.behind === 0) return `Same as ${branch.upstream}`;
  const parts: string[] = [];
  if (branch.ahead > 0) parts.push(`${branch.ahead} ahead`);
  if (branch.behind > 0) parts.push(`${branch.behind} behind`);
  return `${parts.join(", ")} of ${branch.upstream}`;
}

/** "3 files changed, 1 staged": what the work folder holds that no commit has. */
export function changesLine(changes: GitChange[], truncated: boolean): string {
  if (changes.length === 0) return "Nothing changed since the last commit.";
  const staged = changes.filter((c) => c.staged).length;
  const files = `${changes.length}${truncated ? "+" : ""} ${changes.length === 1 ? "file" : "files"} changed`;
  return staged > 0 ? `${files}, ${staged} staged` : files;
}

/** How long ago, short: "5 min", "3 h", "2 d", then the date. */
export function ago(ms: number, now: number = Date.now()): string {
  const minutes = Math.floor((now - ms) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days} d ago`;
  return new Date(ms).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}
