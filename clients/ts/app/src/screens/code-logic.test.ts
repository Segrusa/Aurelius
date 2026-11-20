import type { CodeBranch, CodeCommit, CodeWorktree } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  ago,
  branchPalette,
  branchStatus,
  changesText,
  codeOf,
  dangerAction,
  dirtyText,
  edgePath,
  extraWorktrees,
  fetchedText,
  filterBranches,
  LOOSE_COLOR,
  layoutGraph,
  lostCommitsText,
  MAIN_COLOR,
  mergeBlockedWhy,
  primaryAction,
  repoName,
  STALE_AFTER_MS,
  STATUS_LABEL,
  selectedBranch,
  worktreeAction,
  worktreeCards,
  writtenView,
} from "./code-logic";

const NOW = Date.UTC(2026, 10, 20, 12, 0);
const MIN = 60_000;

function branch(name: string, over: Partial<CodeBranch> = {}): CodeBranch {
  return { name, head: `${name}-head`, ahead: 0, behind: 0, subject: "s", author: "a", at_ms: NOW - 5 * MIN, ...over };
}

function commit(hash: string, parents: string[], branches: string[] = []): CodeCommit {
  return { hash, parents, branches, subject: hash, author: "a", at_ms: NOW };
}

describe("a branch's status", () => {
  it("is Default for main, whatever else is true", () => {
    expect(branchStatus(branch("main", { is_main: true, working: true, ahead: 3 }), NOW)).toBe("default");
  });

  it("is Agent working before merged, stale or ahead", () => {
    expect(branchStatus(branch("f", { working: true, merged: true, at_ms: 0 }), NOW)).toBe("working");
  });

  it("is Merged when every commit is in main, even when it is old", () => {
    expect(branchStatus(branch("f", { merged: true, at_ms: 0 }), NOW)).toBe("merged");
  });

  it("is Stale after 14 days without a commit, not a day earlier", () => {
    const old = branch("f", { ahead: 1, at_ms: NOW - STALE_AFTER_MS - 1 });
    const recent = branch("f", { ahead: 1, at_ms: NOW - STALE_AFTER_MS + 1 });
    expect(branchStatus(old, NOW)).toBe("stale");
    expect(branchStatus(recent, NOW)).toBe("ready");
  });

  it("is Needs rebase when it is ahead and behind, Ready to merge when only ahead", () => {
    expect(branchStatus(branch("f", { ahead: 5, behind: 4 }), NOW)).toBe("behind");
    expect(branchStatus(branch("f", { ahead: 2 }), NOW)).toBe("ready");
  });

  it("is Merged when nothing is ahead of main", () => {
    expect(branchStatus(branch("f", { behind: 3 }), NOW)).toBe("merged");
  });
});

describe("a fresh branch of an open ticket", () => {
  const ticket = { ticket_id: "t80", number: 80, title: "New" };
  const fresh = branch("feat/ticket-80-x", { merged: true, open: true, ticket, worktree_path: "/wt/80" });

  it("a fresh branch of an open ticket is not Merged, stays in Active and cannot be removed from here", () => {
    expect(branchStatus(fresh, NOW)).toBe("fresh");
    expect(STATUS_LABEL.fresh).toBe("Nothing to merge yet");
    // Even when main's last commit (its head) is older than Stale.
    expect(branchStatus({ ...fresh, at_ms: 0 }, NOW)).toBe("fresh");
    expect(filterBranches([fresh], "active", NOW)).toHaveLength(1);
    expect(dangerAction(fresh)).toMatchObject({ kind: "remove_worktree", disabledWhy: "Its ticket is still open." });
    const noWorktree = { ...fresh, worktree_path: null };
    expect(dangerAction(noWorktree)).toMatchObject({ kind: "delete_branch", disabledWhy: "Its ticket is still open." });
    expect(primaryAction(fresh, "fresh", "main")?.kind).toBe("open_ticket");
  });

  it("is Merged again once its ticket is closed, and a branch with commits keeps its status", () => {
    expect(branchStatus({ ...fresh, open: false }, NOW)).toBe("merged");
    expect(dangerAction({ ...fresh, open: false }).disabledWhy).toBeNull();
    expect(branchStatus({ ...fresh, merged: false, ahead: 2 }, NOW)).toBe("ready");
    expect(branchStatus({ ...fresh, merged: false, ahead: 2, behind: 1 }, NOW)).toBe("behind");
  });
});

describe("the branch filters", () => {
  const list = [
    branch("main", { is_main: true }),
    branch("working", { working: true, owner: { employee_id: "e1", name: "Flux" } }),
    branch("ready", { ahead: 1 }),
    branch("merged", { merged: true, owner: { employee_id: "e2", name: "Bridge" } }),
    branch("stale", { ahead: 1, at_ms: 0 }),
  ];
  const names = (f: "active" | "agents" | "all") => filterBranches(list, f, NOW).map((b) => b.name);

  it("Active leaves out Merged and Stale", () => {
    expect(names("active")).toEqual(["main", "working", "ready"]);
  });

  it("By agents keeps only branches an employee owns", () => {
    expect(names("agents")).toEqual(["working", "merged"]);
  });

  it("All keeps everything in order", () => {
    expect(names("all")).toEqual(["main", "working", "ready", "merged", "stale"]);
  });
});

describe("the worktrees", () => {
  const view = {
    worktrees: [
      { path: "/repo", branch: "main", head: "h", is_main: true },
      { path: "/wt/73", branch: "fix/73", head: "h" },
      { path: "/wt/detached", branch: null, head: "h" },
    ],
    branches: [branch("main", { is_main: true }), branch("fix/73", { ahead: 2 })],
  };

  it("count every one but the project's folder", () => {
    expect(extraWorktrees(view).map((w) => w.path)).toEqual(["/wt/73", "/wt/detached"]);
    expect(extraWorktrees(undefined)).toEqual([]);
  });

  it("carry their branch, or none for a detached HEAD", () => {
    const cards = worktreeCards(view);
    expect(cards.map((c) => c.branch?.name ?? null)).toEqual(["fix/73", null]);
  });

  it("say whether they have uncommitted changes", () => {
    expect(dirtyText({ changed_count: 0 })).toBe("Clean");
    expect(dirtyText({ changed_count: 1 })).toBe("1 uncommitted");
    expect(dirtyText({ changed_files: ["a", "b"] })).toBe("2 uncommitted");
    expect(dirtyText({ changed_count: 3, missing: true })).toBe("Missing");
  });
});

describe("a write's answer", () => {
  const view = { project_id: "p", repo_path: "/repo" };

  it("is the new view when it is Code, else nothing", () => {
    expect(codeOf({ kind: "host", value: { type: "code", ...view } })).toEqual(view);
    expect(codeOf({ kind: "command", value: { type: "none" } } as never)).toBeNull();
  });

  it("is used only from a fresh receipt: a replayed one has no lists, so the page reads again", () => {
    const receipt = (replayed: boolean) => ({
      committed_seq: 1,
      idempotency_key: "k",
      replayed,
      result: { kind: "host" as const, value: { type: "code" as const, ...view } },
    });
    expect(writtenView(receipt(false))).toEqual(view);
    expect(writtenView(receipt(true))).toBeNull();
  });
});

describe("the texts", () => {
  it("name the repository after its folder", () => {
    expect(repoName("/home/me/projects/aurelius")).toBe("aurelius");
    expect(repoName("C:\\work\\aurelius\\")).toBe("aurelius");
  });

  it("say how long ago", () => {
    expect(ago(NOW - 10_000, NOW)).toBe("just now");
    expect(ago(NOW - 4 * MIN, NOW)).toBe("4m ago");
    expect(ago(NOW - 2 * 60 * MIN, NOW)).toBe("2h ago");
    expect(ago(NOW - 24 * 60 * MIN, NOW)).toBe("1 day ago");
    expect(ago(NOW - 9 * 24 * 60 * MIN, NOW)).toBe("9 days ago");
  });

  it("say when it was fetched, or why it cannot be", () => {
    expect(fetchedText({ has_remote: true, fetched_at_ms: NOW - 2 * MIN }, NOW)).toBe("Fetched 2m ago");
    expect(fetchedText({ has_remote: true, fetched_at_ms: null }, NOW)).toBe("Never fetched");
    expect(fetchedText({ has_remote: false }, NOW)).toBe("No remote");
  });

  it("sum up the changes", () => {
    expect(changesText({ files_changed: 3, insertions: 41, deletions: 12 })).toBe("3 files · +41 −12");
    expect(changesText({ files_changed: 1, insertions: 1, deletions: 0 })).toBe("1 file · +1 −0");
    expect(changesText({})).toBe("—");
  });

  it("warn how many commits a forced delete loses", () => {
    expect(lostCommitsText({ name: "spike", ahead: 1 })).toBe(
      "spike has 1 commit that main does not have. It is lost when you delete it.",
    );
    expect(lostCommitsText({ name: "spike", ahead: 3 })).toContain("3 commits");
  });
});

describe("the Details card's actions", () => {
  const ticket = { ticket_id: "t1", number: 73, title: "Rail" };

  it("merges a branch that is ready, naming the real main", () => {
    expect(primaryAction(branch("f", { ahead: 2 }), "ready", "master")).toEqual({
      kind: "merge",
      label: "Merge into master",
      disabledWhy: null,
    });
  });

  it("opens the ticket of a working or merged branch, and offers nothing without one", () => {
    expect(primaryAction(branch("f", { ticket }), "working", "main")?.kind).toBe("open_ticket");
    expect(primaryAction(branch("f", { ticket }), "merged", "main")?.kind).toBe("open_ticket");
    expect(primaryAction(branch("f"), "working", "main")).toBeNull();
  });

  it("has no rebase: Needs rebase and Stale only open the ticket", () => {
    expect(primaryAction(branch("f", { ticket }), "behind", "main")?.kind).toBe("open_ticket");
    expect(primaryAction(branch("f"), "stale", "main")).toBeNull();
    expect(primaryAction(branch("main", { is_main: true }), "default", "main")).toBeNull();
  });

  it("removes the worktree of a branch that has one, not while someone works in it", () => {
    expect(dangerAction(branch("f", { worktree_path: "/wt/f" }))).toMatchObject({
      kind: "remove_worktree",
      path: "/wt/f",
      disabledWhy: null,
    });
    expect(dangerAction(branch("f", { worktree_path: "/wt/f", working: true })).disabledWhy).toBeTruthy();
  });

  it("deletes a branch without a worktree, forced only when main lacks its commits", () => {
    expect(dangerAction(branch("f", { merged: true }))).toMatchObject({ kind: "delete_branch", force: false });
    expect(dangerAction(branch("f", { ahead: 1 }))).toMatchObject({ kind: "delete_branch", force: true });
    expect(dangerAction(branch("f", { ahead: 1, working: true })).disabledWhy).toBeTruthy();
  });

  it("cleans up merged branches from main", () => {
    expect(dangerAction(branch("main", { is_main: true }))).toMatchObject({ kind: "prune_merged" });
  });
});

describe("what the server would refuse", () => {
  const repo = (over: Partial<CodeWorktree> = {}) => ({
    repo_path: "/repo",
    main_branch: "main",
    worktrees: [
      { path: "/repo", branch: "main", head: "h", is_main: true, ...over },
      { path: "/wt/f", branch: "f", head: "h", changed_count: 2 },
      { path: "/wt/g", branch: "g", head: "h", changed_files: ["a"] },
      { path: "/wt/clean", branch: "c", head: "h", changed_count: 0 },
      { path: "/wt/detached", branch: null, head: "h", changed_count: 0 },
    ],
  });

  it("Remove worktree is off while the worktree has uncommitted files, with how many", () => {
    expect(dangerAction(branch("f", { worktree_path: "/wt/f" }), repo())).toMatchObject({
      kind: "remove_worktree",
      disabledWhy: "This worktree has 2 uncommitted files.",
    });
    expect(dangerAction(branch("g", { worktree_path: "/wt/g/" }), repo()).disabledWhy).toBe(
      "This worktree has 1 uncommitted file.",
    );
    expect(dangerAction(branch("c", { worktree_path: "/wt/clean" }), repo()).disabledWhy).toBeNull();
  });

  it("a branch checked out in the project folder itself cannot have that folder removed", () => {
    const own = dangerAction(branch("feature", { worktree_path: "/repo" }), repo({ branch: "feature" }));
    expect(own).toMatchObject({ kind: "remove_worktree", disabledWhy: "This is the project's own folder." });
  });

  it("someone working or an open ticket is the first reason given", () => {
    const b = branch("f", { worktree_path: "/wt/f" });
    expect(dangerAction({ ...b, working: true }, repo()).disabledWhy).toBe("An employee works on this branch now.");
    expect(dangerAction({ ...b, open: true }, repo()).disabledWhy).toBe("Its work session is still open.");
    const ticket = { ticket_id: "t", number: 7, title: "T" };
    expect(dangerAction({ ...b, open: true, ticket }, repo()).disabledWhy).toBe("Its ticket is still open.");
  });

  it("a detached worktree can be removed by its path, by the same rules", () => {
    const [, , , , detached] = repo().worktrees;
    expect(worktreeAction(detached as CodeWorktree, repo())).toEqual({
      kind: "remove_worktree",
      label: "Remove worktree",
      path: "/wt/detached",
      disabledWhy: null,
    });
    const dirty = { path: "/wt/d2", branch: null, head: "h", changed_count: 3 };
    expect(worktreeAction(dirty, repo()).disabledWhy).toBe("This worktree has 3 uncommitted files.");
    const busy = { ...(detached as CodeWorktree), working: true };
    expect(worktreeAction(busy, repo()).disabledWhy).toBe("An employee works in this worktree now.");
  });

  it("merging needs the project folder clean and on main", () => {
    expect(mergeBlockedWhy(repo())).toBeNull();
    expect(mergeBlockedWhy(repo({ changed_count: 1 }))).toBe(
      "The project folder has 1 uncommitted file. Commit or discard it first.",
    );
    expect(mergeBlockedWhy(repo({ branch: "spike" }))).toBe("The project folder is on spike, not main.");
    expect(mergeBlockedWhy(repo({ branch: null }))).toBe("The project folder is on a detached HEAD, not main.");
    expect(primaryAction(branch("f", { ahead: 1 }), "ready", "main", repo({ branch: "spike" }))).toEqual({
      kind: "merge",
      label: "Merge into main",
      disabledWhy: "The project folder is on spike, not main.",
    });
  });
});

describe("the branch shown first", () => {
  const list = [branch("main", { is_main: true }), branch("a"), branch("b", { worktree_path: "/wt/b" })];

  it("is the one asked for while it exists", () => {
    expect(selectedBranch(list, "a")?.name).toBe("a");
  });

  it("falls back to a worktree's branch, then main", () => {
    expect(selectedBranch(list, "gone")?.name).toBe("b");
    expect(selectedBranch(list.slice(0, 2), null)?.name).toBe("main");
    expect(selectedBranch([], null)).toBeNull();
  });
});

describe("the commit graph", () => {
  // The design's example, newest first.
  const commits = [
    commit("a3f9", ["7be0"], ["fix/73"]),
    commit("7be0", ["e02b"]),
    commit("91c4", ["4f20"], ["feat/71"]),
    commit("4f20", ["0a9d"]),
    commit("e02b", ["0a9d", "5d8a"], ["main"]),
    commit("5d8a", ["8e1f"], ["fix/64"]),
    commit("8e1f", ["0a9d"]),
    commit("0a9d", ["6c0a"]),
    commit("c71f", ["2b9e"], ["feat/68"]),
    commit("2b9e", ["6c0a"]),
    commit("6c0a", ["older"]),
  ];
  const graph = layoutGraph(commits, "main");
  const laneOf = (h: string) => graph.rows.find((r) => r.commit.hash === h)?.lane;
  const edge = (from: string, to: string) => {
    const f = commits.findIndex((c) => c.hash === from);
    const t = commits.findIndex((c) => c.hash === to);
    return graph.edges.find((e) => e.fromRow === f && e.toRow === t);
  };

  it("keeps main's first-parent chain on lane 0", () => {
    expect(["e02b", "0a9d", "6c0a"].map(laneOf)).toEqual([0, 0, 0]);
  });

  it("gives each other branch tip the next free lane, and its commits follow it", () => {
    expect(["a3f9", "7be0"].map(laneOf)).toEqual([1, 1]);
    expect(["91c4", "4f20"].map(laneOf)).toEqual([2, 2]);
    // Lane 1 ends in the merge, so the merged branch takes it again.
    expect(["5d8a", "8e1f"].map(laneOf)).toEqual([1, 1]);
    // After 0a9d lanes 1 and 2 are free again.
    expect(["c71f", "2b9e"].map(laneOf)).toEqual([1, 1]);
    expect(graph.lanes).toBe(3);
  });

  it("colours a lane after the branch at its tip", () => {
    expect(graph.rows.map((r) => r.branch)).toEqual([
      "fix/73",
      "fix/73",
      "feat/71",
      "feat/71",
      "main",
      "fix/64",
      "fix/64",
      "main",
      "feat/68",
      "feat/68",
      "main",
    ]);
  });

  it("marks a merge commit and draws its merge-in curve out of main", () => {
    expect(graph.rows[4]?.merge).toBe(true);
    expect(edge("e02b", "5d8a")).toMatchObject({ fromLane: 0, via: 1, toLane: 1, branch: "fix/64" });
    // Its first parent stays straight on main.
    expect(edge("e02b", "0a9d")).toMatchObject({ fromLane: 0, via: 0, toLane: 0, branch: "main" });
  });

  it("draws a branch-off curve into main where a branch started", () => {
    expect(edge("7be0", "e02b")).toMatchObject({ fromLane: 1, via: 1, toLane: 0, branch: "fix/73" });
    expect(edge("4f20", "0a9d")).toMatchObject({ fromLane: 2, via: 2, toLane: 0, branch: "feat/71" });
  });

  it("runs a line to a parent outside the list to the bottom", () => {
    expect(graph.edges.find((e) => e.fromRow === 10)).toMatchObject({ cut: true, toRow: 11, toLane: 0, via: 0 });
  });

  it("works without a main branch: the first tip takes lane 0", () => {
    const g = layoutGraph([commit("b", ["a"], ["x"]), commit("a", [])], null);
    expect(g.rows.map((r) => r.lane)).toEqual([0, 0]);
  });

  it("shares a parent between two branches: both lanes end in it", () => {
    const g = layoutGraph(
      [commit("x", ["base"], ["x"]), commit("y", ["base"], ["y"]), commit("base", [], ["main"])],
      "main",
    );
    expect(g.rows.map((r) => r.lane)).toEqual([1, 2, 0]);
    expect(g.edges.map((e) => [e.fromLane, e.toLane])).toEqual([
      [1, 0],
      [2, 0],
    ]);
  });

  it("paths: a straight line, a merge curving out at the top, a branch-off curving in at the bottom", () => {
    expect(edgePath({ fromRow: 0, fromLane: 0, toRow: 1, toLane: 0, via: 0, branch: null, cut: false })).toBe(
      "M12 24 L12 64",
    );
    const merge = edgePath({ fromRow: 0, fromLane: 0, toRow: 2, toLane: 1, via: 1, branch: null, cut: false });
    expect(merge.startsWith("M12 24 C12")).toBe(true);
    expect(merge.endsWith("L28 104")).toBe(true);
    const off = edgePath({ fromRow: 0, fromLane: 1, toRow: 3, toLane: 0, via: 1, branch: null, cut: false });
    expect(off.startsWith("M28 24 L28 122")).toBe(true);
    expect(off.endsWith("12 144")).toBe(true);
  });
});

describe("the colours", () => {
  it("give main grey, a line without a branch a faint one, and up to eight branches each their own", () => {
    const names = ["main", "fix/73", "fix/64", "feat/71", "feat/68", "spike", "a", "b", "c"];
    const color = branchPalette(names, "main");
    expect(color("main")).toBe(MAIN_COLOR);
    expect(color(null)).toBe(LOOSE_COLOR);
    expect(color("unknown")).toBe(LOOSE_COLOR);
    const eight = names.slice(1, 9).map(color);
    expect(new Set(eight).size).toBe(8);
    expect(eight.every((c) => c.startsWith("var(--color-icon-"))).toBe(true);
  });

  it("keep a branch's colour when the list is reordered", () => {
    const one = branchPalette(["main", "x", "y"], "main");
    const other = branchPalette(["y", "main", "x"], "main");
    expect(one("x")).toBe(other("x"));
    expect(one("y")).toBe(other("y"));
  });
});
