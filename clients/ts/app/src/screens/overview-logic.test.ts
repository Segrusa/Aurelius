import type { TicketView, WorkSessionView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import type { Needs } from "./board-logic";
import {
  boardLanes,
  donePerDay,
  elapsed,
  firstLine,
  groupDigits,
  linkedGuidelines,
  needCards,
  needKind,
  recentlyDone,
  runningRows,
  styleguideName,
  workspaceNumbers,
} from "./overview-logic";

const tv = (over: Partial<TicketView> = {}): TicketView => ({
  attachments: [],
  column: "doing",
  column_id: "c-doing",
  created_at_ms: 0,
  created_by_user: "u",
  created_by_username: "Sam",
  id: "t1",
  labels: [],
  login_user: "u",
  login_username: "Sam",
  number: 1,
  owner_name: "Flux",
  owner_employee_id: "e1",
  position: 0,
  priority: "normal",
  project_id: "p",
  project_name: "Aurelius",
  run: {},
  title: "Ticket",
  updated_at_ms: 0,
  version: 1,
  ...over,
});

const ws = (over: Partial<WorkSessionView> = {}): WorkSessionView => ({
  conversation_id: "c1",
  created_at_ms: 0,
  employee_id: "e2",
  employee_name: "Forge",
  last_activity_ms: 0,
  project_id: "p",
  project_name: "Aurelius",
  running: false,
  title: "A session",
  ...over,
});

// 23 Nov 2026, 12:00 local time.
const NOW = new Date(2026, 10, 23, 12, 0).getTime();
const daysAgo = (n: number, hour = 10) => new Date(2026, 10, 23 - n, hour, 0).getTime();

describe("needKind", () => {
  it("makes a review and a plan their own cards and everything else a question", () => {
    expect(needKind("review")).toBe("review");
    expect(needKind("plan_review")).toBe("plan");
    for (const n of ["input", "approval", "failed", "choose_owner", "connect_login"] as Needs[]) {
      expect(needKind(n)).toBe("question");
    }
  });
});

describe("needCards", () => {
  const review = tv({ id: "a", number: 80, title: "iOS tab bar", column: "review", updated_at_ms: 100 });
  const other = tv({ id: "b", number: 81, column: "review", updated_at_ms: 300 });
  const needs = (t: TicketView): Needs | null => (t.column === "review" ? "review" : null);

  it("lists only the tickets that need the reader, and sessions nobody took, newest first", () => {
    const cards = needCards(
      [review, other, tv({ id: "c", archived_at_ms: 5, column: "review" })],
      [ws({ dispatch: "nobody", last_activity_ms: 200 }), ws({ conversation_id: "c2", running: true })],
      needs,
      (t) => t.id !== "b",
    );
    expect(cards.map((c) => c.key)).toEqual(["s:c1", "t:a"]);
    expect(cards[1]).toMatchObject({ kind: "review", title: "#80 iOS tab bar", who: "Flux", to: "/ticket/80" });
    expect(cards[0]).toMatchObject({ kind: "question", to: "/session/c1", conversationId: "c1" });
  });

  it("says what the ticket waits for when no post does", () => {
    const [card] = needCards([review], [], needs, () => true);
    expect(card?.fallback).toBe("Ready for your review.");
  });
});

describe("runningRows", () => {
  it("lists running and queued tickets and running sessions, the longest running first", () => {
    const rows = runningRows(
      [
        tv({ id: "a", number: 83, root_task_status: "running", updated_at_ms: 50 }),
        tv({ id: "b", number: 82, root_task_status: "queued", updated_at_ms: 10 }),
        tv({ id: "c", root_task_status: "waiting" }),
        tv({ id: "d", root_task_status: "running", archived_at_ms: 1 }),
      ],
      [ws({ running: true, last_activity_ms: 30 }), ws({ conversation_id: "c2" })],
      (id) => (id === "a" ? 5 : null),
    );
    expect(rows.map((r) => r.key)).toEqual(["t:a", "t:b", "s:c1"]);
    expect(rows[0]?.sinceMs).toBe(5);
    expect(rows[1]?.queued).toBe(true);
    expect(rows[2]?.number).toBeNull();
  });
});

describe("boardLanes", () => {
  it("counts five lanes, folds plan review and testing into Review, and names the first two", () => {
    const lanes = boardLanes([
      tv({ id: "1", number: 1, column: "backlog", position: 2 }),
      tv({ id: "2", number: 2, column: "backlog", position: 1 }),
      tv({ id: "3", number: 3, column: "backlog", position: 3 }),
      tv({ id: "4", number: 4, column: "plan_review" }),
      tv({ id: "5", number: 5, column: "testing" }),
      tv({ id: "6", number: 6, column: "review" }),
      tv({ id: "7", number: 7, column: "done", done_at_ms: 10 }),
      tv({ id: "8", number: 8, column: "done", done_at_ms: 20 }),
      tv({ id: "9", number: 9, column: "todo", archived_at_ms: 1 }),
    ]);
    expect(lanes.map((l) => [l.name, l.count])).toEqual([
      ["Backlog", 3],
      ["Todo", 0],
      ["Doing", 0],
      ["Review", 3],
      ["Done", 2],
    ]);
    expect(lanes[0]?.top.map((t) => t.number)).toEqual([2, 1]);
    expect(lanes[4]?.top.map((t) => t.number)).toEqual([8, 7]);
  });
});

describe("recentlyDone and donePerDay", () => {
  const done = [
    tv({ id: "a", column: "done", done_at_ms: daysAgo(0) }),
    tv({ id: "b", column: "done", done_at_ms: daysAgo(0, 8) }),
    tv({ id: "c", column: "done", done_at_ms: daysAgo(1) }),
    tv({ id: "d", column: "done", done_at_ms: daysAgo(13) }),
    tv({ id: "e", column: "done", done_at_ms: daysAgo(14) }),
    tv({ id: "f", column: "done", done_at_ms: daysAgo(0), archived_at_ms: 1 }),
    tv({ id: "g", column: "review" }),
  ];

  it("keeps the newest finished tickets, archived ones left out", () => {
    expect(recentlyDone(done, 3).map((t) => t.id)).toEqual(["a", "b", "c"]);
  });

  it("counts per day over 14 days, today last", () => {
    const bars = donePerDay(done, NOW);
    expect(bars).toHaveLength(14);
    expect(bars[13]).toBe(2);
    expect(bars[12]).toBe(1);
    expect(bars[0]).toBe(1);
    expect(bars.reduce((a, b) => a + b, 0)).toBe(4);
  });

  it("gives the numbers of a workspace card", () => {
    expect(workspaceNumbers([...done, tv({ column: "backlog" })], 2, NOW)).toEqual({
      running: 2,
      open: 2,
      review: 1,
      done7: 3,
    });
  });
});

describe("small formats", () => {
  it("sums the guidelines linked to the project only", () => {
    const g = (id: string, chars: number, archived = false) => ({
      links: [{ kind: "project" as const, id }],
      chars,
      archived,
    });
    expect(linkedGuidelines([g("p", 1000), g("p", 240, false), g("q", 5), g("p", 9, true)], "p")).toEqual({
      count: 2,
      chars: 1240,
    });
  });

  it("writes thousands apart with a space", () => {
    expect(groupDigits(11240)).toBe("11 240");
    expect(groupDigits(980)).toBe("980");
  });

  it("names a styleguide after its link", () => {
    expect(styleguideName("https://claude.ai/design/aurelius-precision")).toBe("Aurelius Precision");
    expect(styleguideName("https://claude.ai/")).toBe("claude.ai");
    expect(styleguideName("not a url")).toBe("not a url");
  });

  it("says how long ago", () => {
    expect(elapsed(NOW - 30_000, NOW)).toBe("now");
    expect(elapsed(NOW - 4 * 60_000, NOW)).toBe("4 min");
    expect(elapsed(NOW - 61 * 60_000, NOW)).toBe("1 h");
    expect(elapsed(NOW - 50 * 3_600_000, NOW)).toBe("2 d");
  });

  it("takes the first line of a post without its markdown marks", () => {
    expect(firstLine("\n## Plan ready\nmore")).toBe("Plan ready");
  });
});
