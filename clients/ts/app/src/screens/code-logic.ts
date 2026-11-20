// The Code tab of a workspace (ticket #77): what the page shows of a project's git state, as pure functions.
// The server reads git; the client decides a branch's status, the filters, the actions and the lanes of the
// commit graph, so they are tested here without a repository.

import type { CodeBranch, CodeCommit, CodeView, CodeWorktree } from "@aurelius/protocol";

/** The server feature of the Code tab. */
export const CODE_VIEW = "code_view";
/** A branch without a commit for this long is Stale. */
export const STALE_AFTER_MS = 14 * 24 * 60 * 60 * 1000;
/** The page reads the repository again this often while it is visible. */
export const CODE_REFRESH_MS = 15_000;

export type BranchStatus = "default" | "working" | "merged" | "stale" | "behind" | "ready";

/** The words of each status, as in the design. */
export const STATUS_LABEL: Record<BranchStatus, string> = {
  default: "Default",
  working: "Agent working",
  merged: "Merged",
  stale: "Stale",
  behind: "Needs rebase",
  ready: "Ready to merge",
};

/** Where a branch stands, first match wins: main, working, merged, stale, behind and ahead, ahead. */
export function branchStatus(b: CodeBranch, now: number): BranchStatus {
  if (b.is_main) return "default";
  if (b.working) return "working";
  if (b.merged) return "merged";
  if (now - b.at_ms > STALE_AFTER_MS) return "stale";
  if (b.ahead > 0 && b.behind > 0) return "behind";
  if (b.ahead > 0) return "ready";
  // Nothing ahead of main: all of it is in main, whatever git's merged flag says.
  return "merged";
}

export type BranchFilter = "active" | "agents" | "all";

export const BRANCH_FILTERS: { value: BranchFilter; label: string }[] = [
  { value: "active", label: "Active" },
  { value: "agents", label: "By agents" },
  { value: "all", label: "All" },
];

/** Active: not Merged or Stale. By agents: an employee owns it. All: every branch. */
export function filterBranches(branches: readonly CodeBranch[], filter: BranchFilter, now: number): CodeBranch[] {
  if (filter === "all") return [...branches];
  if (filter === "agents") return branches.filter((b) => b.owner);
  return branches.filter((b) => {
    const s = branchStatus(b, now);
    return s !== "merged" && s !== "stale";
  });
}

/** The worktrees an employee or a person made: every one but the project's folder (the sidebar's count). */
export function extraWorktrees(view: Pick<CodeView, "worktrees"> | undefined): CodeWorktree[] {
  return (view?.worktrees ?? []).filter((w) => !w.is_main);
}

/** "aurelius" for "/home/me/projects/aurelius". */
export function repoName(path: string): string {
  const parts = path.split(/[/\\]/).filter((p) => p !== "");
  return parts[parts.length - 1] ?? path;
}

/** "just now", "4m ago", "2h ago", "9 days ago". */
export function ago(ms: number, now: number): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return d === 1 ? "1 day ago" : `${d} days ago`;
}

/** The head's line about the last fetch. */
export function fetchedText(view: Pick<CodeView, "fetched_at_ms" | "has_remote">, now: number): string {
  if (!view.has_remote) return "No remote";
  if (view.fetched_at_ms === null || view.fetched_at_ms === undefined) return "Never fetched";
  return `Fetched ${ago(view.fetched_at_ms, now)}`;
}

/** "3 files · +41 −12", or a dash when the branch changes nothing. */
export function changesText(b: Pick<CodeBranch, "files_changed" | "insertions" | "deletions">): string {
  const files = b.files_changed ?? 0;
  if (files === 0) return "—";
  return `${files} ${files === 1 ? "file" : "files"} · +${b.insertions ?? 0} −${b.deletions ?? 0}`;
}

/** A worktree's uncommitted state: "Clean", "1 uncommitted", or "Missing" when git lost its folder. */
export function dirtyText(w: Pick<CodeWorktree, "changed_count" | "changed_files" | "missing">): string {
  if (w.missing) return "Missing";
  const n = w.changed_count ?? w.changed_files?.length ?? 0;
  return n === 0 ? "Clean" : `${n} uncommitted`;
}

/** The first seven characters, as git shows a hash. */
export const shortHash = (hash: string) => hash.slice(0, 7);

export type PrimaryAction = { kind: "merge"; label: string } | { kind: "open_ticket"; label: string };

/**
 * The Details card's main button. Ready to merge: merge into main. Otherwise the ticket, when one is known.
 * There is no rebase in v1, so Needs rebase and Stale only show their state.
 */
export function primaryAction(b: CodeBranch, status: BranchStatus, main: string): PrimaryAction | null {
  if (status === "ready") return { kind: "merge", label: `Merge into ${main}` };
  if (status === "default") return null;
  return b.ticket ? { kind: "open_ticket", label: "Open ticket" } : null;
}

export type DangerAction =
  | { kind: "remove_worktree"; label: string; path: string; disabledWhy: string | null }
  | { kind: "delete_branch"; label: string; force: boolean; disabledWhy: string | null }
  | { kind: "prune_merged"; label: string; disabledWhy: null };

const WORKING_WHY = "An employee works on this branch now.";

/**
 * The Details card's danger button. A branch in a worktree: remove the worktree (the branch stays). Another
 * branch: delete it, with force when main lacks some of its commits. Main: clean up the merged branches.
 */
export function dangerAction(b: CodeBranch): DangerAction {
  if (b.is_main) return { kind: "prune_merged", label: "Clean up merged", disabledWhy: null };
  const why = b.working ? WORKING_WHY : null;
  if (b.worktree_path) return { kind: "remove_worktree", label: "Remove worktree", path: b.worktree_path, disabledWhy: why };
  return { kind: "delete_branch", label: "Delete branch", force: !b.merged && b.ahead > 0, disabledWhy: why };
}

/** What the confirm of a forced delete says is lost. */
export function lostCommitsText(b: Pick<CodeBranch, "name" | "ahead">): string {
  const n = b.ahead;
  return `${b.name} has ${n} ${n === 1 ? "commit" : "commits"} that main does not have. ${n === 1 ? "It is" : "They are"} lost when you delete it.`;
}

/** The branch the page shows first: the one asked for if it still exists, else a worktree's, else main. */
export function selectedBranch(branches: readonly CodeBranch[], wanted: string | null): CodeBranch | null {
  return (
    branches.find((b) => b.name === wanted) ??
    branches.find((b) => !b.is_main && b.worktree_path) ??
    branches.find((b) => b.is_main) ??
    branches[0] ??
    null
  );
}

/** A worktree card: the worktree with what its branch knows (owner, ticket, ahead/behind, working). */
export interface WorktreeCard {
  worktree: CodeWorktree;
  branch: CodeBranch | null;
}

export function worktreeCards(view: Pick<CodeView, "worktrees" | "branches">): WorktreeCard[] {
  const byName = new Map((view.branches ?? []).map((b) => [b.name, b]));
  return extraWorktrees(view).map((w) => ({ worktree: w, branch: (w.branch && byName.get(w.branch)) || null }));
}

// The commit graph. Lane 0 is main's first-parent chain; every other line of commits gets the next free lane.

/** One row of the graph. */
export interface GraphRow {
  commit: CodeCommit;
  lane: number;
  /** The branch whose colour the row's lane has; null: a line of commits no listed branch points at. */
  branch: string | null;
  /** Two or more parents. */
  merge: boolean;
}

/**
 * A line from a commit to one of its parents. It leaves the child's lane, runs down in `via`, and enters the
 * parent's lane: a merge curves out at the top, a branch-off curves in at the bottom. `cut`: the parent is
 * not in the list, so the line runs to the bottom of the graph.
 */
export interface GraphEdge {
  fromRow: number;
  fromLane: number;
  toRow: number;
  toLane: number;
  via: number;
  branch: string | null;
  cut: boolean;
}

export interface Graph {
  rows: GraphRow[];
  edges: GraphEdge[];
  lanes: number;
}

/** Main's first-parent chain among the listed commits. */
function mainChain(commits: readonly CodeCommit[], main: string | null): Set<string> {
  const chain = new Set<string>();
  if (!main) return chain;
  const byHash = new Map(commits.map((c) => [c.hash, c]));
  let at = commits.find((c) => c.branches?.includes(main));
  while (at && !chain.has(at.hash)) {
    chain.add(at.hash);
    const first = at.parents?.[0];
    at = first ? byHash.get(first) : undefined;
  }
  return chain;
}

/**
 * Lays out the commits (newest first) in lanes. Each lane waits for one commit (`expects`): the next commit
 * of its line. A commit takes the lowest lane that waits for it (main's chain always lane 0), or the lowest
 * free lane when nothing waits for it (a branch tip). Other lanes that waited for it end in it.
 */
export function layoutGraph(commits: readonly CodeCommit[], main: string | null): Graph {
  const chain = mainChain(commits, main);
  const listed = new Set(commits.map((c) => c.hash));
  const first = chain.size > 0 ? 1 : 0;
  const expects: (string | null)[] = chain.size > 0 ? [null] : [];
  const laneBranch: (string | null)[] = chain.size > 0 ? [main] : [];
  const rows: GraphRow[] = [];
  // Edges wait for their parent's row and lane.
  const open: { fromRow: number; fromLane: number; via: number; parent: string }[] = [];
  const edges: GraphEdge[] = [];
  const placed = new Map<string, { row: number; lane: number }>();

  const freeLane = () => {
    for (let i = first; i < expects.length; i++) if (expects[i] === null) return i;
    expects.push(null);
    laneBranch.push(null);
    return expects.length - 1;
  };

  commits.forEach((c, row) => {
    const waiting = expects.flatMap((h, i) => (h === c.hash ? [i] : []));
    const lane = chain.has(c.hash) ? 0 : (waiting[0] ?? freeLane());
    for (const i of waiting) expects[i] = null;
    // A tip names its lane's branch (main's own name never leaves lane 0).
    const tip = (c.branches ?? []).find((b) => b !== main);
    if (lane !== 0 || chain.size === 0) {
      if (tip) laneBranch[lane] = tip;
      else if (waiting.length === 0) laneBranch[lane] = null;
    }
    rows.push({ commit: c, lane, branch: laneBranch[lane] ?? null, merge: (c.parents?.length ?? 0) > 1 });
    placed.set(c.hash, { row, lane });

    (c.parents ?? []).forEach((parent, n) => {
      let via: number;
      if (n === 0) {
        via = lane;
        if (listed.has(parent)) expects[lane] = parent;
      } else {
        // A lane that already waits for the parent takes the line; else a new one does.
        const already = expects.indexOf(parent);
        if (already !== -1) via = already;
        else {
          via = freeLane();
          laneBranch[via] = null;
          if (listed.has(parent)) expects[via] = parent;
        }
      }
      open.push({ fromRow: row, fromLane: lane, via, parent });
    });
  });

  const rowOf = new Map(rows.map((r) => [r.commit.hash, r]));
  for (const e of open) {
    const to = placed.get(e.parent);
    const parentRow = to ? rowOf.get(e.parent) : undefined;
    // A first-parent line keeps its own colour; a merge line has the colour of what it brings in.
    const branch = e.via === e.fromLane ? (rows[e.fromRow]?.branch ?? null) : (parentRow?.branch ?? null);
    edges.push({
      fromRow: e.fromRow,
      fromLane: e.fromLane,
      toRow: to ? to.row : commits.length,
      toLane: to ? to.lane : e.via,
      via: e.via,
      branch,
      cut: !to,
    });
  }
  const lanes = Math.max(1, ...rows.map((r) => r.lane + 1), ...edges.map((e) => e.via + 1));
  return { rows, edges, lanes };
}

/** The graph's measures: the design's 40 px rows, 16 px between lanes. */
export const GRAPH = { row: 40, lane: 16, left: 12, top: 4 } as const;

export const laneX = (lane: number) => GRAPH.left + lane * GRAPH.lane;
export const rowY = (row: number) => GRAPH.top + row * GRAPH.row + GRAPH.row / 2;

/** The SVG path of an edge: out of the child's lane at the top, down `via`, into the parent's lane at the bottom. */
export function edgePath(e: GraphEdge): string {
  const x1 = laneX(e.fromLane);
  const xv = laneX(e.via);
  const x2 = laneX(e.toLane);
  const y1 = rowY(e.fromRow);
  const y2 = e.cut ? GRAPH.top + e.toRow * GRAPH.row : rowY(e.toRow);
  const curves = (e.via !== e.fromLane ? 1 : 0) + (e.via !== e.toLane ? 1 : 0);
  // Two curves in one row share its height.
  const h = curves === 0 ? 0 : Math.min(22, (y2 - y1) / curves);
  let d = `M${x1} ${y1}`;
  let y = y1;
  if (e.via !== e.fromLane) {
    d += ` C${x1} ${y1 + h * 0.64} ${xv} ${y1 + h * 0.36} ${xv} ${y1 + h}`;
    y = y1 + h;
  }
  if (e.via !== e.toLane) {
    if (y2 - h > y) d += ` L${xv} ${y2 - h}`;
    d += ` C${xv} ${y2 - h * 0.36} ${x2} ${y2 - h * 0.64} ${x2} ${y2}`;
  } else {
    d += ` L${xv} ${y2}`;
  }
  return d;
}

/** The graph's full height. */
export const graphHeight = (rows: number) => GRAPH.top * 2 + rows * GRAPH.row;

/**
 * Lane colours from the theme's icon palette. Main is grey; another branch keeps its colour across the
 * worktrees, the table and the graph because it comes from its name.
 */
const BRANCH_COLORS = [
  "var(--color-icon-green)",
  "var(--color-icon-purple)",
  "var(--color-icon-blue)",
  "var(--color-icon-yellow)",
  "var(--color-icon-pink)",
  "var(--color-icon-teal)",
  "var(--color-icon-orange)",
  "var(--color-icon-cyan)",
];
export const MAIN_COLOR = "var(--color-icon-gray)";
export const LOOSE_COLOR = "var(--color-icon-disabled)";

export function branchColor(name: string | null, main: string | null): string {
  if (name === null) return LOOSE_COLOR;
  if (name === main) return MAIN_COLOR;
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) | 0;
  return BRANCH_COLORS[Math.abs(hash) % BRANCH_COLORS.length] as string;
}
