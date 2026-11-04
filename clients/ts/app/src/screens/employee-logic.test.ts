import type { AgentView, EmployeeSummary } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  canPin,
  employeeGroups,
  matchesSearch,
  orgTree,
  pinnedFirst,
  position,
  reportsCandidates,
  threadGroup,
  togglePin,
  workOf,
} from "./employee-logic";

const m = (name: string, boss: string | null = null, over: Partial<EmployeeSummary> = {}) =>
  ({ id: name, name, role: "Role", status: "active", reports_to: boss, ...over }) as EmployeeSummary;

const names = (groups: { label: string; list: EmployeeSummary[] }[]) =>
  groups.map((g) => [g.label, g.list.map((e) => e.name)]);

describe("search and names", () => {
  it("looks in name, role and title", () => {
    const s = m("Simon", null, { title: "Lead server" });
    for (const q of ["", "simon", "SERVER", "role"]) expect(matchesSearch(s, q)).toBe(true);
    expect(matchesSearch(s, "mobile")).toBe(false);
    expect(position(s)).toBe("Lead server · Role");
    expect(position(m("A"))).toBe("Role");
  });
});

describe("groups", () => {
  it("a paused root is among the others", () => {
    const all = [m("Atlas", null, { status: "paused" }), m("Titan", "Atlas"), m("Flux", "Titan")];
    expect(names(employeeGroups(all, all, []))).toEqual([
      ["REPORTS TO ATLAS", ["Titan"]],
      ["REPORTS TO TITAN", ["Flux"]],
      ["OTHERS", ["Atlas"]],
    ]);
    expect(
      names(
        employeeGroups(
          all,
          all.filter((e) => matchesSearch(e, "atl")),
          [],
        ),
      ),
    ).toEqual([["OTHERS", ["Atlas"]]]);
  });
  it("pins go first", () => {
    const all = [m("Atlas"), m("Nova", null, { status: "paused" }), m("Titan", "Atlas"), m("Flux", "Titan")];
    expect(names(employeeGroups(all, all, ["Flux"]))).toEqual([
      ["PINNED", ["Flux"]],
      ["MANAGEMENT TEAM", ["Atlas"]],
      ["REPORTS TO ATLAS", ["Titan"]],
      ["OTHERS", ["Nova"]],
    ]);
  });
});

describe("org chart", () => {
  it("builds the tree", () => {
    const all = [m("Dean", "Simon"), m("Tom"), m("Simon", "Jean"), m("Max", "Simon"), m("Jean")];
    const flat = (nodes: ReturnType<typeof orgTree>, d = 0): [number, string][] =>
      nodes.flatMap((n) => [[d, n.employee.name] as [number, string], ...flat(n.children, d + 1)]);
    expect(flat(orgTree(all))).toEqual([
      [0, "Tom"],
      [0, "Jean"],
      [1, "Simon"],
      [2, "Dean"],
      [2, "Max"],
    ]);
  });
  it("a boss who left leaves a root", () => {
    const all = [m("Boss", null, { status: "archived" }), m("Report", "Boss"), m("Elsewhere")];
    expect(orgTree(all).map((n) => n.employee.name)).toEqual(["Report", "Elsewhere"]);
  });
  it("offers no one below as boss", () => {
    const all = [m("Jean"), m("Simon", "Jean"), m("Dean", "Simon")];
    expect(reportsCandidates(all, "Jean").map((e) => e.name)).toEqual([]);
    expect(reportsCandidates(all, "Simon").map((e) => e.name)).toEqual(["Jean"]);
    expect(reportsCandidates(all, null).map((e) => e.name)).toEqual(["Jean", "Simon", "Dean"]);
  });
});

describe("pins", () => {
  it("orders, toggles and limits", () => {
    const list = ["a", "b", "c", "d", "e"].map((id) => ({ id }));
    expect(pinnedFirst(list, ["c", "gone", "a"]).map((x) => x.id)).toEqual(["c", "a", "b", "d", "e"]);
    let pins: string[] = [];
    for (const id of ["b", "a", "c", "a"]) pins = togglePin(pins, id);
    expect(pins).toEqual(["b", "c"]);
    expect(canPin(["1", "2", "3", "4"], "x")).toBe(true);
    expect(canPin(["1", "2", "3", "4", "5"], "x")).toBe(false);
    expect(canPin(["1", "2", "3", "4", "5"], "1")).toBe(true);
  });
});

describe("work", () => {
  const a = (kind: string, name: string, detail: string, since_ms: number) =>
    ({ kind, name, detail, since_ms, steps: 0 }) as AgentView;
  it("prefers task work, then a conversation, then colleagues", () => {
    expect(workOf([])).toBeNull();
    expect(workOf([a("subagent", "Explore", "", 1)])).toBeNull();
    const agents = [
      a("delegated", "Bram", "Pricing · running", 1),
      a("conversation", "Roadmap chat", "", 2),
      a("planning", "Planning", "Landing page", 3),
    ];
    expect(workOf(agents)).toEqual({ title: "Landing page", since: 3 });
    expect(workOf(agents.slice(0, 2))).toEqual({ title: "Roadmap chat", since: 2 });
    expect(workOf(agents.slice(0, 1))).toEqual({ title: "Waiting on Bram: Pricing · running", since: 1 });
  });
  it("groups a thread by how old it is", () => {
    const now = 1_759_665_600_000;
    const h = 3_600_000;
    expect(threadGroup(true, false, now - 720 * h, now)).toBe("active");
    expect(threadGroup(true, false, now - h, now)).toBe("active");
    expect(threadGroup(false, false, now - h, now)).toBe("past");
    expect(threadGroup(false, false, now - 30 * h, now)).toBe("earlier");
    expect(threadGroup(false, true, now - 30 * h, now)).toBe("past");
  });
});
