import type { ChannelView, GuidelineProposalView, ProjectSummary } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  guidelineProposalsIn,
  guidelineReach,
  guidelinesIn,
  homeChannel,
  inWorkspace,
  narrow,
  openProjects,
  pathAfterSwitch,
  resolveWorkspace,
  storedWorkspace,
  targetOf,
  workspaceAfterOpening,
  workspaceKey,
} from "./workspace-logic";

const project = (id: string, status: ProjectSummary["status"] = "active") =>
  ({ id, name: id, status, created_at_ms: 0, employees_active: 0, version: 1 }) as ProjectSummary;
const channel = (over: Partial<ChannelView>) => ({ conversation_id: "c", name: "c", ...over }) as ChannelView;
const onProject = (id: string) => ({ kind: "project", id }) as const;
const onEmployee = (id: string) => ({ kind: "employee", id }) as const;

const atlas = project("atlas");
const gone = project("gone", "archived");
const projects = [atlas, project("zephyr"), gone];

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

  it("opens on the project's channel; general only under All projects", () => {
    const general = channel({ conversation_id: "g", kind: "general" });
    const own = channel({ conversation_id: "a", project_id: "atlas" });
    const thread = channel({ conversation_id: "t", project_id: "atlas", ticket_id: "t1" });
    const list = [general, thread, own];
    expect(homeChannel(list, null)).toBe(general);
    expect(homeChannel(list, "atlas")).toBe(own);
    expect(homeChannel(list, "zephyr")).toBeNull();
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
    expect(targetOf("/chat", { channel: "c1" })).toEqual({ kind: "channel", id: "c1" });
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
    expect(pathAfterSwitch("/chat", "atlas")).toBe("/chat");
    expect(pathAfterSwitch("/tasks/k1", "atlas")).toBe("/tasks");
  });

  it("follows the settings page of a workspace", () => {
    expect(pathAfterSwitch("/projects/atlas", "zephyr")).toBe("/projects/zephyr");
    expect(pathAfterSwitch("/projects/atlas", null)).toBe("/projects");
  });

  it("stays on a screen that has no opened item", () => {
    expect(pathAfterSwitch("/inbox", "atlas")).toBeNull();
    expect(pathAfterSwitch("/tasks", "atlas")).toBeNull();
  });
});
