import { describe, expect, it } from "vitest";
import { giveWorkParams, LAST_GIVEN, ticketCreateParams } from "./dialog-logic";
import {
  COMPOSER_ROWS_MAX,
  COMPOSER_ROWS_MIN,
  columnLabel,
  composerRows,
  continueTickets,
  createDrafts,
  DISPATCHER,
  defaultKind,
  defaultProject,
  defaultWho,
  footerLine,
  noteRecent,
  PROJECT_ROWS,
  parseRecent,
  placeholderOf,
  projectRows,
  remembered,
  type SessionStart,
  sessionKey,
  sessionRequest,
  shownProjects,
  startProblem,
} from "./new-session-logic";
import { LAST_QUICK, QUICK_NOTE, quickCreateParams } from "./quick-logic";

const project = (id: string, status = "active") => ({ id, name: id.toUpperCase(), status });
const employee = (id: string, status = "active") => ({ id, name: `Name ${id}`, status });

describe("the projects New session shows", () => {
  const all = [project("a"), project("b"), project("old", "archived")];

  it("are the open ones, or only the workspace's project when one is chosen", () => {
    expect(shownProjects(all, null).map((p) => p.id)).toEqual(["a", "b"]);
    expect(shownProjects(all, "b").map((p) => p.id)).toEqual(["b"]);
    // A workspace whose project is gone narrows nothing.
    expect(shownProjects(all, "gone").map((p) => p.id)).toEqual(["a", "b"]);
  });

  it("default to the workspace's, else the last used, else the only one, else none (§4.1)", () => {
    const two = shownProjects(all, null);
    expect(defaultProject(two, "b", "a")).toBe("b");
    expect(defaultProject(two, null, "a")).toBe("a");
    expect(defaultProject(two, null, "old")).toBeNull();
    expect(defaultProject(two, null, null)).toBeNull();
    expect(defaultProject([project("a")], null, null)).toBe("a");
    expect(defaultProject([], null, "a")).toBeNull();
  });

  it("list the first six by last use, with More when there are others", () => {
    const many = ["p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"].map((id) => project(id));
    const { rows, more } = projectRows(many, ["p8", "p3"], null);
    expect(rows.map((p) => p.id)).toEqual(["p8", "p3", "p1", "p2", "p4", "p5"]);
    expect(rows).toHaveLength(PROJECT_ROWS);
    expect(more).toBe(true);
    expect(projectRows(many.slice(0, 3), [], null)).toEqual({ rows: many.slice(0, 3), more: false });
  });

  it("keep the chosen project in view when it was picked from All projects", () => {
    const many = ["p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"].map((id) => project(id));
    expect(projectRows(many, [], "p8").rows.map((p) => p.id)).toEqual(["p1", "p2", "p3", "p4", "p5", "p8"]);
  });

  it("remember the latest use first, without repeats, and read a damaged list as none", () => {
    expect(noteRecent(["a", "b", "c"], "c")).toEqual(["c", "a", "b"]);
    expect(parseRecent(JSON.stringify(["a", "b"]))).toEqual(["a", "b"]);
    expect(parseRecent("not json")).toEqual([]);
    expect(parseRecent(JSON.stringify({ a: 1 }))).toEqual([]);
    expect(parseRecent(null)).toEqual([]);
  });
});

describe("the kind and who", () => {
  const team = [employee("e1"), employee("e2"), employee("gone", "archived")];

  it("is a ticket unless a quick task was last used and the server has quick tasks (§4.4)", () => {
    expect(defaultKind(null, true)).toBe("ticket");
    expect(defaultKind("quick", true)).toBe("quick");
    expect(defaultKind("quick", false)).toBe("ticket");
    expect(defaultKind("nonsense", true)).toBe("ticket");
  });

  it("goes to the dispatcher for a ticket, or to the employee last chosen while still active", () => {
    expect(defaultWho("ticket", null, team, null)).toBe(DISPATCHER);
    expect(defaultWho("ticket", "e2", team, null)).toBe("e2");
    expect(defaultWho("ticket", "gone", team, null)).toBe(DISPATCHER);
    expect(defaultWho("ticket", DISPATCHER, team, null)).toBe(DISPATCHER);
  });

  it("goes to the last quick-task employee for a quick task, else to no one: the board member chooses", () => {
    expect(defaultWho("quick", "e1", team, "e2")).toBe("e2");
    expect(defaultWho("quick", DISPATCHER, team, null)).toBeNull();
    expect(defaultWho("quick", null, team, "gone")).toBeNull();
  });

  it("is kept per server, under its own keys", () => {
    expect(sessionKey("project", "host-1")).toBe("new_session.project:host-1");
    expect(sessionKey("who", null)).toBe("new_session.who:");
  });

  it("is remembered with the dialogs' own last choice, so both open on the same person", () => {
    expect(remembered("host-1", { kind: "ticket", projectId: "p", who: DISPATCHER }, ["x"])).toEqual([
      ["new_session.project:host-1", "p"],
      ["new_session.recent:host-1", JSON.stringify(["p", "x"])],
      ["new_session.kind:host-1", "ticket"],
      ["new_session.who:host-1", DISPATCHER],
    ]);
    expect(remembered("host-1", { kind: "ticket", projectId: "p", who: "e1" }, [])).toContainEqual([LAST_GIVEN, "e1"]);
    const quick = remembered("host-1", { kind: "quick", projectId: "p", who: "e1" }, []);
    expect(quick).toContainEqual([LAST_QUICK, "e1"]);
    // A quick task leaves the ticket's who as it was.
    expect(quick.some(([key]) => key === "new_session.who:host-1")).toBe(false);
  });
});

describe("what Start sends (§4.6)", () => {
  const base: SessionStart = {
    kind: "ticket",
    projectId: "p-1",
    text: "  Fix the login\nIt fails on Safari  ",
    who: { id: DISPATCHER, name: "Dispatcher" },
    model: "default",
    effort: "default",
    mode: "default",
    files: [],
    responsibleUserId: "u-me",
  };

  it("for the dispatcher is New ticket's create and start, titled by the first line", () => {
    const { params, done } = sessionRequest(base, false);
    expect(params).toEqual(
      ticketCreateParams(
        {
          projectId: "p-1",
          title: "Fix the login",
          description: "  Fix the login\nIt fails on Safari  ",
          priority: "normal",
          labels: [],
          deadlineMs: null,
          files: [],
          start: true,
          run: {},
          responsibleUserId: "u-me",
          ownerEmployeeId: null,
        },
        false,
      ),
    );
    expect(params).toEqual({
      project_id: "p-1",
      title: "Fix the login",
      description: "Fix the login\nIt fails on Safari",
      priority: "normal",
      labels: [],
      deadline_ms: null,
      start: true,
      run: {},
      confirm_secret: false,
      responsible_user_id: "u-me",
    });
    expect(done).toBe("Ticket created and started.");
  });

  it("for the dispatcher carries the chosen mode, model and effort", () => {
    const { params } = sessionRequest(
      { ...base, mode: "plan", model: "m-1", effort: "high", files: ["/f"], responsibleUserId: null },
      true,
    );
    expect(params.run).toEqual({ mode: "plan", model: "m-1", effort: "high" });
    expect(params.files).toEqual(["/f"]);
    expect(params.confirm_secret).toBe(true);
    expect("responsible_user_id" in params).toBe(false);
  });

  it("for an employee is exactly Give work's request", () => {
    const start = { ...base, who: { id: "e-1", name: "Flux" }, mode: "auto", effort: "low", files: ["/a"] };
    const { params, done } = sessionRequest(start, true);
    expect(params).toEqual(
      giveWorkParams(
        {
          projectId: "p-1",
          goal: start.text,
          employeeId: "e-1",
          run: { mode: "auto", effort: "low" },
          files: ["/a"],
          responsibleUserId: "u-me",
        },
        true,
      ),
    );
    expect(done).toBe("Created and started with Flux.");
  });

  it("for a quick task is exactly the dialogs' quick task, without a mode", () => {
    const start: SessionStart = { ...base, kind: "quick", who: { id: "e-1", name: "Flux" }, mode: "plan" };
    const { params, done } = sessionRequest({ ...start, model: "m-1", effort: "max" }, false);
    expect(params).toEqual(
      quickCreateParams(
        {
          projectId: "p-1",
          title: "",
          description: start.text,
          employeeId: "e-1",
          files: [],
          model: "m-1",
          effort: "max",
        },
        false,
      ),
    );
    expect(params.run?.mode).toBeUndefined();
    expect(done).toBe("Quick task started with Flux.");
  });
});

describe("whether Start can go", () => {
  const ready = { projectId: "p", text: "Do it", kind: "ticket" as const, who: DISPATCHER, offline: false };

  it("needs a project, a text and, for a quick task, an employee", () => {
    expect(startProblem(ready)).toBeNull();
    expect(startProblem({ ...ready, projectId: null })).toBe("Choose a project.");
    expect(startProblem({ ...ready, text: "  \n " })).toBe("Describe what needs doing.");
    expect(startProblem({ ...ready, kind: "quick", who: null })).toBe("Choose who does it.");
    expect(startProblem({ ...ready, kind: "quick", who: DISPATCHER })).toBe("Choose who does it.");
    expect(startProblem({ ...ready, kind: "quick", who: "e1" })).toBeNull();
  });

  it("waits for the server", () => {
    expect(startProblem({ ...ready, offline: true })).toBe("Not connected to the server.");
  });
});

describe("the line above the chips (§4.5)", () => {
  it("says what happens next, in the dialogs' words", () => {
    expect(footerLine({ kind: "ticket", whoName: null, responsible: true, noConnection: false })).toEqual({
      type: "text",
      text: "The dispatcher picks the owner. You are responsible.",
    });
    expect(footerLine({ kind: "ticket", whoName: null, responsible: false, noConnection: false })).toEqual({
      type: "text",
      text: "The dispatcher picks the owner.",
    });
    expect(footerLine({ kind: "ticket", whoName: "Flux", responsible: true, noConnection: false })).toEqual({
      type: "text",
      text: "Flux reports back to you when it is done.",
    });
    expect(footerLine({ kind: "quick", whoName: "Flux", responsible: true, noConnection: false })).toEqual({
      type: "text",
      text: QUICK_NOTE,
    });
  });

  it("says first that nothing can start without a connection", () => {
    expect(footerLine({ kind: "quick", whoName: "Flux", responsible: true, noConnection: true })).toEqual({
      type: "no_connection",
    });
  });
});

describe("the composer", () => {
  it("asks for what the kind needs", () => {
    expect(placeholderOf("ticket")).toBe("Describe a ticket or give work…");
    expect(placeholderOf("quick")).toBe("What needs doing? Starts right away on its own branch");
  });

  it("grows from one line up to ten, then scrolls", () => {
    expect(composerRows("")).toBe(COMPOSER_ROWS_MIN);
    expect(composerRows("one line")).toBe(COMPOSER_ROWS_MIN);
    expect(composerRows("a\nb\nc")).toBe(3);
    expect(composerRows("x\n".repeat(30))).toBe(COMPOSER_ROWS_MAX);
  });

  it("keeps a draft per server until a start clears it", () => {
    const drafts = createDrafts();
    drafts.set("h1", "half a thought");
    drafts.set("h2", "other server");
    expect(drafts.get("h1")).toBe("half a thought");
    drafts.clear("h1");
    expect(drafts.get("h1")).toBe("");
    expect(drafts.get("h2")).toBe("other server");
    expect(drafts.get(null)).toBe("");
  });
});

describe("the tickets to continue (§4.2)", () => {
  const ticket = (id: string, column: string, updated: number) => ({
    id,
    number: Number(id.slice(1)),
    title: id,
    column,
    updated_at_ms: updated,
  });

  it("are at most three in Doing, Testing or Review, newest activity first", () => {
    const list = [
      ticket("t1", "doing", 10),
      ticket("t2", "review", 40),
      ticket("t3", "backlog", 99),
      ticket("t4", "testing", 30),
      ticket("t5", "done", 98),
      ticket("t6", "doing", 20),
    ];
    expect(continueTickets(list).map((t) => t.id)).toEqual(["t2", "t4", "t6"]);
    expect(continueTickets([])).toEqual([]);
  });

  it("show their column in words", () => {
    expect(["doing", "testing", "review"].map(columnLabel)).toEqual(["Doing", "Testing", "Review"]);
  });
});
