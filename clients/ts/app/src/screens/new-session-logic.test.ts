import type { Receipt } from "@aurelius/protocol";
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
  defaultDesign,
  defaultKind,
  defaultProject,
  defaultWho,
  keptChoice,
  noteRecent,
  offeredKind,
  PROJECT_ROWS,
  parseRecent,
  placeholderOf,
  placeLabel,
  placeOptions,
  projectRows,
  remembered,
  type SessionStart,
  sessionDispatchParams,
  sessionKey,
  sessionRequest,
  sessionStartParams,
  shownPlace,
  shownProjects,
  startedSession,
  startProblem,
} from "./new-session-logic";
import { LAST_QUICK, quickCreateParams } from "./quick-logic";

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

  it("remembers the model, effort and mode a start used, so the next one opens on them", () => {
    const run = { model: "claude-opus-5-5", effort: "high", mode: "plan" };
    const writes = remembered("host-1", { kind: "session", projectId: "p", who: "e1", run }, []);
    expect(writes).toContainEqual(["new_session.model:host-1", "claude-opus-5-5"]);
    expect(writes).toContainEqual(["new_session.effort:host-1", "high"]);
    expect(writes).toContainEqual(["new_session.mode:host-1", "plan"]);
  });

  it("opens on a remembered choice only while it is still offered", () => {
    expect(keptChoice("high", ["default", "low", "high"])).toBe("high");
    expect(keptChoice("gone-model", ["default", "m-1"])).toBe("default");
    expect(keptChoice(null, ["default", "m-1"])).toBe("default");
    // Before the models have loaded, the remembered one is kept rather than lost.
    expect(keptChoice("m-2", ["default"], false)).toBe("m-2");
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

  it("leaves the title empty on a server that makes it from the description", () => {
    const made = { ...base, madeTitle: true };
    const forDispatcher = sessionRequest(made, false).params;
    expect(forDispatcher.title).toBe("");
    expect(forDispatcher.description).toBe("Fix the login\nIt fails on Safari");
    const forEmployee = sessionRequest({ ...made, who: { id: "e-1", name: "Flux" } }, false).params;
    expect(forEmployee).toEqual({
      ...sessionRequest({ ...base, who: { id: "e-1", name: "Flux" } }, false).params,
      title: "",
    });
    // An older server refuses an empty title: it gets the first line.
    expect(sessionRequest({ ...base, madeTitle: false }, false).params.title).toBe("Fix the login");
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

describe("work sessions", () => {
  it("offers Session next to Ticket where the server has them, else Quick task", () => {
    expect(offeredKind(["quick_task", "work_sessions"], true)).toBe("session");
    expect(offeredKind(["quick_task"], true)).toBe("quick");
    expect(offeredKind([], false)).toBeNull();
  });

  it("starts as a Session when a session or a quick task was used last", () => {
    expect(defaultKind("session", true, true)).toBe("session");
    expect(defaultKind("quick", true, true)).toBe("session");
    expect(defaultKind("ticket", true, true)).toBe("ticket");
    expect(defaultKind(null, true, true)).toBe("ticket");
  });

  it("needs an employee and takes files only once it has started", () => {
    const ready = { projectId: "p", text: "fix the login", kind: "session" as const, who: "e", offline: false };
    expect(startProblem(ready)).toBeNull();
    expect(startProblem({ ...ready, who: DISPATCHER })).toBe("Choose who does it.");
    expect(startProblem({ ...ready, files: 1 })).toBe("Add files in the session once it has started.");
  });

  it("takes files with its first message on a server with chat_files, but not in its own worktree", () => {
    const ready = { projectId: "p", text: "fix the login", kind: "session" as const, who: "e", offline: false };
    expect(startProblem({ ...ready, files: 1, sessionFiles: true })).toBeNull();
    expect(startProblem({ ...ready, files: 1, sessionFiles: true, worktree: true })).toContain("own worktree");
    expect(startProblem({ ...ready, files: 0, sessionFiles: false, worktree: true })).toBeNull();
  });

  it("asks what to work on, without a ticket", () => {
    expect(placeholderOf("session")).toContain("without a ticket");
  });

  it("sends the employee, the project, the worktree choice and only the chosen run settings", () => {
    const start = {
      projectId: "p",
      employeeId: "e",
      text: "  fix it  ",
      model: "default",
      effort: "high",
      mode: "auto",
    };
    expect(sessionStartParams(start, "worktree", false)).toEqual({
      project_id: "p",
      employee_id: "e",
      text: "fix it",
      worktree: true,
      run: { mode: "auto", model: null, effort: "high" },
      confirm_secret: false,
    });
    expect(sessionStartParams({ ...start, mode: "default" }, "branch", true).run).toEqual({
      model: null,
      effort: "high",
    });
  });

  it("sends photos and files only when there are some", () => {
    const start = {
      projectId: "p",
      employeeId: "e",
      text: "kijk",
      model: "default",
      effort: "default",
      mode: "default",
    };
    expect(sessionStartParams({ ...start, images: [], files: [] }, "branch", false)).not.toHaveProperty("images");
    expect(sessionStartParams({ ...start, images: [], files: [] }, "branch", false)).not.toHaveProperty("files");
    const sent = sessionStartParams({ ...start, images: ["/a.png"], files: ["/b.pdf"] }, "branch", false);
    expect([sent.images, sent.files]).toEqual([["/a.png"], ["/b.pdf"]]);
  });

  it("reads the started session from the receipt", () => {
    const receipt = (value: object) => ({ result: { kind: "command", value } }) as unknown as Receipt;
    expect(
      startedSession(
        receipt({ type: "work_session_started", conversation_id: "c", project_id: "p", message_id: "m", run_id: "r" }),
      ),
    ).toBe("c");
    expect(startedSession(receipt({ type: "ticket_created", ticket_id: "t" }))).toBeNull();
  });
});

describe("where a session works (session_own_branch)", () => {
  it("offers Own branch only where the server has it", () => {
    expect(placeOptions(false).map((o) => o.key)).toEqual(["worktree"]);
    expect(placeOptions(true).map((o) => [o.key, o.label])).toEqual([
      ["branch", "Own branch"],
      ["worktree", "Own worktree"],
    ]);
    expect(placeLabel("branch")).toBe("Own branch");
  });

  it("never works in the project folder as it is: own branch unless chosen, else a worktree", () => {
    expect(shownPlace(null, true)).toBe("branch");
    expect(shownPlace("worktree", true)).toBe("worktree");
    expect(shownPlace(null, false)).toBe("worktree");
    expect(shownPlace("branch", false)).toBe("worktree");
  });

  it("names Own branch as the way to send files only where the server has it", () => {
    const ready = { projectId: "p", text: "fix", kind: "session" as const, who: "e", offline: false };
    const files = { ...ready, files: 1, sessionFiles: true, worktree: true };
    expect(startProblem({ ...files, ownBranch: true })).toBe(
      "Add files once the session works in its own worktree, or start it on its own branch.",
    );
    expect(startProblem(files)).toBe("Add files once the session works in its own worktree.");
  });

  it("sends own_branch for a branch of its own, and nothing new otherwise", () => {
    const start = {
      projectId: "p",
      employeeId: "e",
      text: "fix it",
      model: "default",
      effort: "default",
      mode: "default",
    };
    const own = sessionStartParams(start, "branch", false);
    expect([own.worktree, own.own_branch]).toEqual([false, true]);
    expect("own_branch" in sessionStartParams(start, "worktree", false)).toBe(false);
    expect(sessionDispatchParams(start, "branch", false).own_branch).toBe(true);
  });

  it("takes files with the first message on its own branch", () => {
    const ready = { projectId: "p", text: "fix", kind: "session" as const, who: "e", offline: false };
    expect(startProblem({ ...ready, files: 1, sessionFiles: true, worktree: false })).toBeNull();
  });
});

describe("a session the dispatcher gives to someone (session_dispatch)", () => {
  const team = [employee("e1"), employee("e2", "paused")];

  it("goes to the dispatcher by default, or to the employee chosen last while still active", () => {
    expect(defaultWho("session", null, team, null, true)).toBe(DISPATCHER);
    expect(defaultWho("session", null, team, DISPATCHER, true)).toBe(DISPATCHER);
    expect(defaultWho("session", null, team, "e2", true)).toBe(DISPATCHER);
    expect(defaultWho("session", null, team, "e1", true)).toBe("e1");
    // A server without it: the board member chooses, as before.
    expect(defaultWho("session", null, team, null, false)).toBeNull();
  });

  it("may start with the dispatcher only where the server has it", () => {
    const ready = { projectId: "p", text: "fix the login", kind: "session" as const, who: DISPATCHER, offline: false };
    expect(startProblem({ ...ready, sessionDispatch: true })).toBeNull();
    expect(startProblem({ ...ready, sessionDispatch: false })).toBe("Choose who does it.");
    expect(startProblem({ ...ready, kind: "quick", sessionDispatch: true })).toBe("Choose who does it.");
  });

  it("sends a session start without an employee, and reads the session from its receipt", () => {
    const start = { projectId: "p", text: " fix it ", model: "sonnet", effort: "default", mode: "default" };
    expect(sessionDispatchParams(start, "branch", false)).toEqual({
      project_id: "p",
      text: "fix it",
      worktree: false,
      own_branch: true,
      run: { model: "sonnet", effort: null },
      confirm_secret: false,
    });
    const receipt = { result: { kind: "command", value: { type: "work_session_dispatching", conversation_id: "c" } } };
    expect(startedSession(receipt as unknown as Receipt)).toBe("c");
  });
});

describe("a design", () => {
  const first = {
    projectId: "p1",
    employeeId: "e1",
    text: "A pricing page",
    model: "default",
    effort: "default",
    mode: "default",
  };

  it("is a session that asks for a design, and leaves the flag out otherwise", () => {
    expect(sessionStartParams({ ...first, design: true }, "branch", false).design).toBe(true);
    expect("design" in sessionStartParams(first, "branch", false)).toBe(false);
    expect(sessionDispatchParams({ ...first, design: true }, "branch", false).design).toBe(true);
  });

  it("is chosen again next time, as a session, only where the server has designs", () => {
    expect(defaultKind("design", true, true)).toBe("session");
    expect(defaultDesign("design", true)).toBe(true);
    expect(defaultDesign("design", false)).toBe(false);
    expect(defaultDesign("session", true)).toBe(false);
  });

  it("is remembered as a design, and asks what it should look like", () => {
    const writes = remembered("h1", { kind: "session", design: true, projectId: "p1", who: "e1" }, []);
    expect(writes).toContainEqual(["new_session.kind:h1", "design"]);
    expect(placeholderOf("session", true)).toMatch(/look like/);
  });
});
