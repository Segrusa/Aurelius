import type { GuidelineProposalView, ProjectSummary } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { areasOf } from "../data";
import {
  byName,
  chosenRail,
  guidelineProposalsIn,
  guidelineReach,
  guidelinesIn,
  inWorkspace,
  narrow,
  openProjects,
  pathAfterSwitch,
  railPins,
  railProjects,
  resolveWorkspace,
  startPath,
  storedWorkspace,
  targetOf,
  workspaceAfterOpening,
  workspaceKey,
} from "./workspace-logic";

const project = (id: string, status: ProjectSummary["status"] = "active") =>
  ({ id, name: id, status, created_at_ms: 0, employees_active: 0, version: 1 }) as ProjectSummary;
const onProject = (id: string) => ({ kind: "project", id }) as const;
const onEmployee = (id: string) => ({ kind: "employee", id }) as const;

const atlas = project("atlas");
const gone = project("gone", "archived");
const projects = [atlas, project("zephyr"), gone];

describe("the rail's workspaces", () => {
  const open = [atlas, project("zephyr"), project("mira")];
  const ids = (list: ProjectSummary[]) => list.map((p) => p.id);

  it("pins every open workspace until this device chooses, the old stars first", () => {
    expect(railPins(null, open, [])).toEqual(["atlas", "zephyr", "mira"]);
    expect(railPins(null, open, ["mira", "gone"])).toEqual(["mira", "atlas", "zephyr"]);
  });

  it("keeps exactly what this device chose, even nothing", () => {
    expect(railPins(["zephyr"], open, ["mira"])).toEqual(["zephyr"]);
    expect(railPins([], open, [])).toEqual([]);
  });

  it("shows only pinned open workspaces, in pin order, skipping an archived pin", () => {
    expect(ids(railProjects(["mira", "gone", "atlas"], openProjects([...open, gone]), null))).toEqual([
      "mira",
      "atlas",
    ]);
  });

  it("adds the current workspace at the end while it is not pinned", () => {
    expect(ids(railProjects(["atlas"], open, "mira"))).toEqual(["atlas", "mira"]);
    expect(ids(railProjects(["atlas"], open, "atlas"))).toEqual(["atlas"]);
  });

  it("takes the board member's rail from the server when it keeps rails, and this device's otherwise", () => {
    expect(chosenRail(true, { chosen: true, project_ids: ["mira"] }, ["atlas"])).toEqual(["mira"]);
    expect(chosenRail(true, { chosen: true, project_ids: [] }, ["atlas"])).toEqual([]);
    expect(chosenRail(false, undefined, ["atlas"])).toEqual(["atlas"]);
    expect(chosenRail(false, undefined, null)).toBeNull();
  });

  it("rereads only the rail when a board member's rail changed", () => {
    expect(areasOf("user.rail_pins_changed")).toEqual(["rail"]);
    expect(areasOf("user.created")).not.toContain("rail");
  });

  it("counts a rail the board member never chose as unchosen, and an unloaded one as unknown", () => {
    expect(chosenRail(true, { chosen: false, project_ids: [] }, ["atlas"])).toBeNull();
    expect(chosenRail(true, undefined, ["atlas"])).toBeUndefined();
  });

  it("lists All workspaces by name, whatever the case", () => {
    const named = (id: string, name: string) => ({ ...project(id), name });
    expect(ids(byName([named("b", "beta"), named("a", "Alpha"), named("c", "Céline")]))).toEqual(["a", "b", "c"]);
  });
});

describe("the current workspace", () => {
  it("is remembered per server, and empty means All projects", () => {
    expect(workspaceKey("h1")).not.toBe(workspaceKey("h2"));
    expect(storedWorkspace(null)).toBeNull();
    expect(storedWorkspace("")).toBeNull();
    expect(storedWorkspace("atlas")).toBe("atlas");
  });

  it("does not offer an archived project", () => {
    expect(openProjects(projects).map((p) => p.id)).toEqual(["atlas", "zephyr"]);
  });

  it("falls back to All projects when its project is archived or removed", () => {
    expect(resolveWorkspace("atlas", projects)).toBe("atlas");
    expect(resolveWorkspace("gone", projects)).toBeNull();
    expect(resolveWorkspace("removed", projects)).toBeNull();
  });

  it("waits for the projects before it gives up on one", () => {
    expect(resolveWorkspace("atlas", undefined)).toBe("atlas");
  });
});

describe("what a workspace shows", () => {
  const rows = [{ project_id: "atlas" }, { project_id: "zephyr" }, { project_id: null }, {}];

  it("shows everything under All projects", () => {
    expect(narrow(rows, null)).toBe(rows);
    expect(inWorkspace(null, null)).toBe(true);
  });

  it("shows only its own project, and nothing without a project", () => {
    expect(narrow(rows, "atlas")).toEqual([{ project_id: "atlas" }]);
    expect(inWorkspace("atlas", null)).toBe(false);
    expect(inWorkspace("atlas", undefined)).toBe(false);
  });
});

describe("the guidelines of a workspace", () => {
  const employees = [
    { id: "here", projects_active: ["atlas"] },
    { id: "there", projects_active: ["zephyr"] },
  ];

  it("names why a guideline applies", () => {
    expect(guidelineReach({ links: [onProject("atlas")] }, "atlas", employees)).toBe("project");
    expect(guidelineReach({ links: [onEmployee("here")] }, "atlas", employees)).toBe("employee");
    expect(guidelineReach({ links: [] }, "atlas", employees)).toBe("company");
    expect(guidelineReach({ links: [onEmployee("there")] }, "atlas", employees)).toBe("company");
  });

  it("leaves out what is only another project's", () => {
    const other = { links: [onProject("zephyr")] };
    const own = { links: [onProject("atlas")] };
    expect(guidelineReach(other, "atlas", employees)).toBeNull();
    expect(guidelinesIn([other, own], "atlas", employees)).toEqual([own]);
    expect(guidelinesIn([other, own], null, employees)).toEqual([other, own]);
  });

  it("counts a proposal for the project it suggests", () => {
    const here = { suggested_links: [onProject("atlas")] } as unknown as GuidelineProposalView;
    const none = {} as unknown as GuidelineProposalView;
    expect(guidelineProposalsIn([here, none], "atlas")).toEqual([here]);
    expect(guidelineProposalsIn([here, none], null)).toEqual([here, none]);
  });
});

describe("following a link", () => {
  it("reads what a location points at", () => {
    expect(targetOf("/board", { ticket: "t1" })).toEqual({ kind: "ticket", id: "t1" });
    expect(targetOf("/tasks/k1", {})).toEqual({ kind: "task", id: "k1" });
    expect(targetOf("/knowledge/memory", { project: "atlas" })).toEqual({ kind: "project", id: "atlas" });
  });

  it("finds nothing on a plain screen", () => {
    expect(targetOf("/board", {})).toBeNull();
    expect(targetOf("/board", { ticket: "" })).toBeNull();
    expect(targetOf("/tasks", {})).toBeNull();
    expect(targetOf("/inbox", { ticket: "t1" })).toBeNull();
  });

  it("switches to the project of what it opens", () => {
    expect(workspaceAfterOpening("zephyr", "atlas", projects)).toBe("atlas");
  });

  it("stays where the thing already shows", () => {
    expect(workspaceAfterOpening(null, "atlas", projects)).toBeNull();
    expect(workspaceAfterOpening("atlas", "atlas", projects)).toBe("atlas");
  });

  it("goes to All projects for what no open workspace can show", () => {
    expect(workspaceAfterOpening("atlas", null, projects)).toBeNull();
    expect(workspaceAfterOpening("atlas", "gone", projects)).toBeNull();
  });
});

describe("switching under an open screen", () => {
  it("drops the opened item of the old workspace", () => {
    expect(pathAfterSwitch("/board", "atlas")).toBe("/board");
    expect(pathAfterSwitch("/tasks/k1", "atlas")).toBe("/tasks");
    // A ticket's session is of one project: the Board of the new workspace instead.
    expect(pathAfterSwitch("/ticket/70", "atlas")).toBe("/board");
  });

  it("follows the settings page of a workspace", () => {
    expect(pathAfterSwitch("/projects/atlas", "zephyr")).toBe("/projects/zephyr");
    expect(pathAfterSwitch("/projects/atlas", null)).toBe("/projects");
  });

  it("stays on a screen that has no opened item", () => {
    expect(pathAfterSwitch("/inbox", "atlas")).toBeNull();
    expect(pathAfterSwitch("/tasks", "atlas")).toBeNull();
  });

  it("opens the list of workspaces on All workspaces, which has no panel to leave a workspace's screen", () => {
    expect(pathAfterSwitch("/board", null)).toBe("/overview");
    expect(pathAfterSwitch("/new", null)).toBe("/overview");
    expect(pathAfterSwitch("/ticket/70", null)).toBe("/overview");
    // Settings have their own sidebar, so they stay.
    expect(pathAfterSwitch("/settings/account", null)).toBeNull();
    expect(pathAfterSwitch("/projects", null)).toBeNull();
  });
});

describe("where the app starts", () => {
  it("starts on the list of workspaces under All workspaces, otherwise on New session", () => {
    expect(startPath(true, null)).toBe("/overview");
    expect(startPath(true, "atlas")).toBe("/new");
    // A server without workspaces has no list: New session, as before.
    expect(startPath(false, null)).toBe("/new");
  });
});
