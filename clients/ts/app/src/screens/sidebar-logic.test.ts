import { describe, expect, it } from "vitest";
import {
  bucketed,
  bucketOf,
  EARLIER_MAX,
  foldKey,
  isFolded,
  needsPerProject,
  type SidebarItem,
  sectionKey,
  workspaceGroups,
} from "./sidebar-logic";

const DAY = 1_000_000;

function item(key: string, over: Partial<SidebarItem> = {}): SidebarItem {
  return {
    key,
    section: "sessions",
    to: `/session/${key}`,
    label: key,
    who: "Flux",
    whoId: "e1",
    projectId: "a",
    signal: null,
    at: DAY + 10,
    ...over,
  };
}

describe("the sidebar's buckets", () => {
  it("puts working rows under Running, today's under Today and older ones under Earlier", () => {
    expect(bucketOf({ signal: "working", at: 0 }, DAY)).toBe("running");
    expect(bucketOf({ signal: null, at: DAY }, DAY)).toBe("today");
    expect(bucketOf({ signal: null, at: DAY - 1 }, DAY)).toBe("earlier");
  });

  it("keeps an old row that waits for you under Today, so it is never folded away", () => {
    expect(bucketOf({ signal: "needs_you", at: 0 }, DAY)).toBe("today");
  });

  it("lists only non-empty buckets, newest first, and caps Earlier", () => {
    const old = Array.from({ length: EARLIER_MAX + 5 }, (_, i) => item(`old${i}`, { at: i }));
    const list = bucketed([item("t1", { at: DAY + 1 }), item("t2", { at: DAY + 2 }), ...old], DAY);
    expect(list.map((b) => b.bucket)).toEqual(["today", "earlier"]);
    expect(list[0]?.items.map((i) => i.key)).toEqual(["t2", "t1"]);
    expect(list[1]?.items).toHaveLength(EARLIER_MAX);
    expect(list[1]?.items[0]?.key).toBe(`old${EARLIER_MAX + 4}`);
  });
});

describe("All workspaces", () => {
  it("groups per project in the rail's order, then what has no project, and skips empty projects", () => {
    const groups = workspaceGroups(
      [
        item("a1", { projectId: "a", signal: "working" }),
        item("b1", { projectId: "b" }),
        item("n1", { projectId: null }),
        item("gone", { projectId: "archived" }),
      ],
      ["b", "a", "c"],
      DAY,
    );
    expect(groups.map((g) => g.id)).toEqual(["b", "a", null]);
    expect(groups[1]).toMatchObject({ count: 1, running: 1 });
    expect(groups[2]?.buckets[0]?.items.map((i) => i.key)).toEqual(["n1", "gone"]);
  });

  it("counts per project what waits for you, for the rail's badges", () => {
    const counts = needsPerProject([
      { projectId: "a", signal: "needs_you" },
      { projectId: "a", signal: "needs_you" },
      { projectId: "a", signal: "working" },
      { projectId: null, signal: "needs_you" },
    ]);
    expect([...counts]).toEqual([["a", 2]]);
  });
});

describe("folds", () => {
  it("keeps one key per fold, per workspace under All workspaces", () => {
    expect(foldKey(undefined, "today")).toBe("sidebar.fold.today");
    expect(foldKey("p1")).toBe("sidebar.fold.p1");
    expect(foldKey(null, "earlier")).toBe("sidebar.fold.none.earlier");
  });

  it("keeps the Tickets section's folds apart from the Sessions section's", () => {
    expect(foldKey(undefined, "today", "tickets")).toBe("sidebar.tickets.fold.today");
    expect(foldKey(null, "earlier", "tickets")).toBe("sidebar.tickets.fold.none.earlier");
    expect(sectionKey("sessions")).toBe("sidebar.sessions");
    expect(sectionKey("tickets")).toBe("sidebar.tickets");
  });

  it("opens Earlier only on request and the rest until folded", () => {
    expect(isFolded(null, "earlier")).toBe(true);
    expect(isFolded(null, "today")).toBe(false);
    expect(isFolded(null)).toBe(false);
    expect(isFolded("open", "earlier")).toBe(false);
    expect(isFolded("folded", "running")).toBe(true);
  });
});
