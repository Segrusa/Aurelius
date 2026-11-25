import type { CodeBranch, CodeWorktree, TicketView, WorkSessionView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  behindText,
  canSubmit,
  currentItem,
  filterQueue,
  mainlineText,
  mergeBlocked,
  mergeCheck,
  type ReviewItem,
  reviewCheck,
  reviewPost,
  reviewQueue,
  sizeCheck,
  tidyText,
  tidyUp,
  waited,
} from "./code-review-logic";

const NOW = Date.UTC(2026, 10, 25, 12, 0);
const MIN = 60_000;
const HOUR = 60 * MIN;

const branch = (name: string, over: Partial<CodeBranch> = {}): CodeBranch => ({
  name,
  head: `${name}-head`,
  ahead: 2,
  behind: 0,
  subject: "s",
  author: "a",
  at_ms: NOW - HOUR,
  ...over,
});

const tv = (over: Partial<TicketView> = {}): TicketView => ({
  attachments: [],
  column: "review",
  column_id: "c-review",
  created_at_ms: 0,
  created_by_user: "u",
  created_by_username: "Sam",
  id: "t83",
  labels: [],
  login_user: "u",
  login_username: "Sam",
  number: 83,
  owner_name: "Flux",
  owner_employee_id: "e-flux",
  position: 0,
  priority: "normal",
  project_id: "p",
  project_name: "Aurelius",
  run: {},
  title: "Ticket page: Activity and Timeline",
  updated_at_ms: NOW - 2 * HOUR,
  version: 1,
  ...over,
});

const ws = (over: Partial<WorkSessionView> = {}): WorkSessionView => ({
  conversation_id: "c1",
  created_at_ms: 0,
  employee_id: "e-titan",
  employee_name: "Titan",
  last_activity_ms: NOW - 25 * MIN,
  project_id: "p",
  project_name: "Aurelius",
  running: false,
  title: "Design sessions always start in Auto",
  branch: "session/35f38c72",
  ...over,
});

const wt = (path: string, b: string, changed = 0): CodeWorktree => ({
  path,
  branch: b,
  head: "h",
  changed_count: changed,
});

const ticketBranch = (name: string, t: TicketView, over: Partial<CodeBranch> = {}) =>
  branch(name, { ticket: { ticket_id: t.id, number: t.number, title: t.title }, ...over });

describe("the review queue", () => {
  it("lists a ticket in Review whose branch is ahead of main", () => {
    const t = tv();
    const q = reviewQueue({ branches: [ticketBranch("feat/ticket-83", t)] }, [t], []);
    expect(q.ready.map((i) => [i.key, i.kind, i.number, i.ownerName])).toEqual([
      ["feat/ticket-83", "ticket", 83, "Flux"],
    ]);
  });

  it("leaves out a ticket that is not in Review yet", () => {
    const t = tv({ column: "doing" });
    expect(reviewQueue({ branches: [ticketBranch("feat/ticket-83", t)] }, [t], []).ready).toEqual([]);
  });

  it("leaves out a branch with nothing ahead of main and nothing uncommitted", () => {
    const t = tv();
    expect(reviewQueue({ branches: [ticketBranch("f", t, { ahead: 0 })] }, [t], []).ready).toEqual([]);
  });

  it("lists an idle session on its own branch, with uncommitted work only", () => {
    const s = ws();
    const q = reviewQueue(
      { branches: [branch("session/35f38c72", { ahead: 0 })], worktrees: [wt("/repo", "session/35f38c72", 5)] },
      [],
      [s],
    );
    expect(q.ready.map((i) => [i.kind, i.title, i.uncommitted, i.where])).toEqual([
      ["session", "Design sessions always start in Auto", 5, "/repo"],
    ]);
  });

  it("leaves out a session whose run is live, and lists its branch under Being worked on", () => {
    const s = ws({ running: true });
    const q = reviewQueue({ branches: [branch("session/35f38c72", { working: true })] }, [], [s]);
    expect(q.ready).toEqual([]);
    expect(q.working.map((i) => i.key)).toEqual(["session/35f38c72"]);
  });

  it("never lists main or a branch of neither a ticket nor a session", () => {
    const q = reviewQueue(
      { branches: [branch("main", { is_main: true }), branch("loose", { working: true })] },
      [],
      [],
    );
    expect(q).toEqual({ ready: [], working: [] });
  });

  it("puts the oldest waiting first, across tickets and sessions", () => {
    const old = tv({ id: "t82", number: 82, updated_at_ms: NOW - 24 * HOUR });
    const fresh = tv({ id: "t83", number: 83, updated_at_ms: NOW - 2 * HOUR });
    const s = ws({ last_activity_ms: NOW - 3 * HOUR });
    const q = reviewQueue(
      {
        branches: [
          ticketBranch("feat/ticket-83", fresh),
          branch("session/35f38c72"),
          ticketBranch("feat/ticket-82", old),
        ],
      },
      [old, fresh],
      [s],
    );
    expect(q.ready.map((i) => i.key)).toEqual(["feat/ticket-82", "session/35f38c72", "feat/ticket-83"]);
  });

  it("filters on tickets or sessions", () => {
    const t = tv();
    const q = reviewQueue({ branches: [ticketBranch("feat/ticket-83", t), branch("session/35f38c72")] }, [t], [ws()]);
    expect(filterQueue(q.ready, "session").map((i) => i.kind)).toEqual(["session"]);
    expect(filterQueue(q.ready, "ticket").map((i) => i.kind)).toEqual(["ticket"]);
    expect(filterQueue(q.ready, "all")).toHaveLength(2);
  });

  it("keeps the item asked for while it is listed, else shows the first", () => {
    const t = tv();
    const { ready } = reviewQueue({ branches: [ticketBranch("a", t), branch("session/35f38c72")] }, [t], [ws()]);
    expect(currentItem(ready, "session/35f38c72")?.key).toBe("session/35f38c72");
    expect(currentItem(ready, "gone")?.key).toBe(ready[0]?.key);
    expect(currentItem([], "gone")).toBeNull();
  });
});

describe("tidy up", () => {
  it("names merged branches nobody needs and the worktrees they leave behind", () => {
    const view = {
      branches: [
        branch("main", { is_main: true, ahead: 0 }),
        branch("feat/done", { ahead: 0, merged: true }),
        branch("feat/open", { ahead: 0, merged: true, open: true }),
        branch("feat/live", { ahead: 3 }),
      ],
      worktrees: [
        { ...wt("/repo", "main"), is_main: true },
        wt("/wt/done", "feat/done"),
        { ...wt("/wt/lost", "feat/x"), missing: true },
        wt("/wt/live", "feat/live"),
      ],
    };
    expect(tidyUp(view, NOW)).toEqual({ branches: ["feat/done"], worktrees: ["/wt/done", "/wt/lost"] });
  });

  it("leaves a worktree of a merged branch alone while it holds uncommitted work", () => {
    const view = {
      branches: [branch("feat/done", { ahead: 0, merged: true })],
      worktrees: [wt("/wt/done", "feat/done", 1)],
    };
    expect(tidyUp(view, NOW).worktrees).toEqual([]);
  });

  it("says what is left, or nothing when all is tidy", () => {
    expect(tidyText({ branches: ["a", "b"], worktrees: ["w"] }, "main")).toBe(
      "2 branches are merged into main, 1 worktree is left behind.",
    );
    expect(tidyText({ branches: [], worktrees: [] }, "main")).toBeNull();
  });
});

function item(over: Partial<ReviewItem> = {}): ReviewItem {
  return {
    key: "feat/ticket-83",
    kind: "ticket",
    branch: branch("feat/ticket-83", { files_changed: 9, insertions: 612, deletions: 148, ahead: 3 }),
    title: "T",
    number: 83,
    ticket: null,
    session: null,
    ownerId: "e",
    ownerName: "Flux",
    since: NOW,
    uncommitted: 0,
    where: null,
    ...over,
  };
}

describe("the checks", () => {
  it("shows the board member's own verdict before anything else", () => {
    expect(reviewCheck(item(), "approved")).toMatchObject({ tone: "ok", value: "Approved" });
    expect(reviewCheck(item(), "changes")).toMatchObject({ tone: "warn", value: "Changes requested" });
    expect(reviewCheck(item({ kind: "session" }), null)).toMatchObject({ value: "Not reviewed" });
  });

  it("warns on uncommitted work before anything git says about main", () => {
    expect(mergeCheck(item({ uncommitted: 5 }), "main")).toMatchObject({
      tone: "warn",
      value: "Not committed",
      sub: "5 files changed since the last commit",
    });
  });

  it("is green when the branch is up to date with main and says how far main moved on otherwise", () => {
    expect(mergeCheck(item(), "main")).toMatchObject({ tone: "ok", value: "Up to date" });
    const behind = item({ branch: branch("f", { behind: 2 }) });
    expect(mergeCheck(behind, "main")).toMatchObject({ tone: "warn", value: "2 behind" });
    expect(behindText(behind.branch, "main")).toBe("2 behind main");
  });

  it("gives the size of what a merge brings", () => {
    expect(sizeCheck(item().branch, 0)).toMatchObject({ value: "9 files", sub: "+612 −148 · 3 commits" });
    expect(sizeCheck(branch("f", { files_changed: 1, ahead: 1 }), 2).sub).toBe("+0 −0 · 1 commit + changes");
  });

  it("turns Merge off while work is not committed or nothing is ahead", () => {
    expect(mergeBlocked(item())).toBeNull();
    expect(mergeBlocked(item({ uncommitted: 1 }))).toBe("Flux has to commit first.");
    expect(mergeBlocked(item({ branch: branch("f", { ahead: 0 }) }))).toBe("Nothing to merge.");
  });
});

describe("finishing a review", () => {
  it("needs words for a comment or a request for changes, not for an approval", () => {
    expect(canSubmit("approved", "")).toBe(true);
    expect(canSubmit("comment", "  ")).toBe(false);
    expect(canSubmit("changes", "fix it")).toBe(true);
  });

  it("posts the verdict with the summary", () => {
    expect(reviewPost("approved", "", "feat/x")).toBe("Approved feat/x.");
    expect(reviewPost("changes", "Dedupe it.", "feat/x")).toBe("Changes requested on feat/x:\n\nDedupe it.");
    expect(reviewPost("comment", " Looks fine ", "feat/x")).toBe("Looks fine");
  });
});

describe("waited", () => {
  it("reads in minutes, hours and days", () => {
    expect(waited(NOW - 25 * MIN, NOW)).toBe("25 min");
    expect(waited(NOW - 2 * HOUR, NOW)).toBe("2 h");
    expect(waited(NOW - 30 * HOUR, NOW)).toBe("1 d");
    expect(waited(NOW, NOW)).toBe("now");
  });
});

describe("the head's line about main", () => {
  it("says how far main is from its remote and when it was fetched", () => {
    const main = branch("main", { is_main: true, ahead: 0, upstream: "origin/main", upstream_ahead: 4 });
    expect(
      mainlineText({ branches: [main], main_branch: "main", has_remote: true, fetched_at_ms: NOW - 12 * MIN }, NOW),
    ).toBe("main · 4 ahead of origin · fetched 12 min ago");
    expect(mainlineText({ branches: [], main_branch: "main", has_remote: false }, NOW)).toBe("main · no remote");
  });
});
