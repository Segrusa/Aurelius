import type { AgentView, ConnectionsSnapshot, ConnectionView, EmployeeSummary } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  accessLabel,
  accessOf,
  canPin,
  choiceLine,
  employeeGroups,
  inheritedRun,
  matchesSearch,
  modelAfterProvider,
  modelsFor,
  orgTree,
  pinnedFirst,
  position,
  recentChats,
  reportsCandidates,
  threadGroup,
  togglePin,
  usableConnections,
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

describe("what an employee runs on", () => {
  const conn = (id: string, models: string[], over: Partial<ConnectionView> = {}) =>
    ({ id, display_name: `Conn ${id}`, models, status: "ready", ...over }) as ConnectionView;
  const all = [conn("a", ["opus", "sonnet"], { default_model: "sonnet" }), conn("b", ["gpt"])];

  it("offers the models of the chosen connection, or all of them on Inherit", () => {
    expect(modelsFor(all, "inherit")).toEqual(["opus", "sonnet", "gpt"]);
    expect(modelsFor(all, "b")).toEqual(["gpt"]);
  });

  it("drops a model the new connection does not have", () => {
    expect(modelAfterProvider(all, "b", "opus")).toBe("inherit");
    expect(modelAfterProvider(all, "a", "opus")).toBe("opus");
    expect(modelAfterProvider(all, "b", "inherit")).toBe("inherit");
  });

  it("never offers a disconnected connection", () => {
    expect(usableConnections([...all, conn("c", [], { status: "disconnected" })]).map((c) => c.id)).toEqual(["a", "b"]);
  });

  it("says what Inherit comes to from the company's default", () => {
    const company = (settings: object) =>
      ({
        connections: all,
        defaults: [{ purpose: "employee", scope: { kind: "global" }, settings }],
      }) as unknown as ConnectionsSnapshot;
    expect(inheritedRun(undefined)).toEqual({ provider: null, model: null, effort: null });
    expect(inheritedRun({ connections: all })).toEqual({ provider: null, model: null, effort: null });
    expect(inheritedRun(company({ connection: { mode: "value", value: "a" } }))).toEqual({
      provider: "Conn a",
      model: "sonnet",
      effort: null,
    });
    expect(
      inheritedRun(
        company({
          connection: { mode: "value", value: "a" },
          model: { mode: "value", value: "opus" },
          effort: { mode: "value", value: "high" },
        }),
      ),
    ).toEqual({ provider: "Conn a", model: "opus", effort: "high" });
  });

  it("reads a choice as its value, or as Inherit with what that comes to", () => {
    expect(choiceLine("inherit", null)).toBe("Inherit");
    expect(choiceLine("inherit", "Sonnet")).toBe("Inherit · Sonnet");
    expect(choiceLine("opus", "Sonnet", (v) => v.toUpperCase())).toBe("OPUS");
  });

  it("treats an employee without a level as Inherit and names an unknown one", () => {
    expect(accessOf(null)).toBe("inherit");
    expect(accessOf(m("A"))).toBe("inherit");
    expect(accessOf(m("A", null, { access_level: "read_only" }))).toBe("read_only");
    expect(accessLabel("read_edit")).toBe("Read and edit");
    expect(accessLabel("unknown")).toBe("Unknown");
  });
});

describe("recentChats", () => {
  it("keeps the order and shows at most four", () => {
    expect(recentChats([1, 2, 3, 4, 5, 6])).toEqual([1, 2, 3, 4]);
    expect(recentChats([2, 1])).toEqual([2, 1]);
    expect(recentChats([])).toEqual([]);
  });
});
