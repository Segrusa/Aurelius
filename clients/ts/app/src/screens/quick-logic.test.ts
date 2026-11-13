import { describe, expect, it } from "vitest";
import { ticketCreateParams } from "./dialog-logic";
import {
  acceptLabel,
  cancelConfirmation,
  cancelNeedsConfirm,
  doneConfirmation,
  doneLine,
  isQuick,
  kindFilterOptions,
  kindMatches,
  quickAssignee,
  quickCreateParams,
  quickEnabled,
  quickProblem,
  quickTag,
  shortCommit,
} from "./quick-logic";

const quick = { number: 7, kind: "quick" };
const normal: { number: number; kind?: string | null } = { number: 8 };

describe("the quick_task feature", () => {
  it("is on only when the server names it", () => {
    expect(quickEnabled(["ticket_follow", "quick_task"])).toBe(true);
    expect(quickEnabled(["ticket_follow"])).toBe(false);
    expect(quickEnabled([])).toBe(false);
  });

  it("hides the tag and the filter on a server without it", () => {
    expect(quickTag(quick, false)).toBeNull();
    expect(kindFilterOptions(false)).toEqual([]);
  });
});

describe("a quick task", () => {
  it("is a ticket whose kind is quick; a normal one has none", () => {
    expect(isQuick(quick)).toBe(true);
    expect(isQuick(normal)).toBe(false);
    expect(isQuick({ kind: null })).toBe(false);
    expect(isQuick({ kind: "normal" })).toBe(false);
  });

  it("is tagged Quick on the card and in the lists", () => {
    expect(quickTag(quick, true)).toBe("Quick");
    expect(quickTag(normal, true)).toBeNull();
  });

  it("is what the Quick tasks filter keeps", () => {
    expect(kindFilterOptions(true).map((o) => o.label)).toEqual(["All tickets", "Quick tasks"]);
    expect([quick, normal].filter((t) => kindMatches("quick", t))).toEqual([quick]);
    expect([quick, normal].filter((t) => kindMatches("all", t))).toEqual([quick, normal]);
  });

  it("is approved and merged in Review; a normal ticket is marked done", () => {
    expect(acceptLabel(quick)).toBe("Approve and merge");
    expect(acceptLabel(normal)).toBe("Mark done");
    expect(doneConfirmation(quick).confirm).toBe("Approve and merge");
    expect(doneConfirmation(quick).body).toContain("merged into the project's main");
    expect(doneConfirmation(normal).confirm).toBe("Mark done");
  });

  it("always asks before Cancel or a drag to Backlog, and says the work is lost", () => {
    expect(cancelNeedsConfirm(quick, false)).toBe(true);
    expect(cancelNeedsConfirm(normal, false)).toBe(false);
    expect(cancelNeedsConfirm(normal, true)).toBe(true);
    expect(cancelConfirmation(quick)).toEqual({
      title: "Cancel quick task #7?",
      body: "Its branch and worktree are removed; work that was not merged is lost.",
      confirm: "Cancel quick task",
    });
    expect(cancelConfirmation(normal).confirm).toBe("Back to Backlog");
  });

  it("shows its merge commit, short, once Done", () => {
    expect(shortCommit("0123456789abcdef")).toBe("0123456");
    expect(doneLine({ kind: "quick", merge_commit: "0123456789abcdef" }, "14:02")).toBe(
      "Done · 14:02. Merged into main as 0123456. Nothing was pushed.",
    );
    expect(doneLine({ kind: "quick" }, null)).toBe("Done. The branch had no commits, so nothing was merged.");
    expect(doneLine({}, null)).toBe("Done. Nothing was merged, pushed or deployed by closing it.");
  });
});

describe("creating a quick task", () => {
  const form = {
    projectId: "p1",
    title: "  ",
    description: " Fix the typo on the login page \n",
    employeeId: "e1",
    files: [],
    model: null,
  };

  it("sends kind quick with its employee and description, and no start or mode", () => {
    const params = quickCreateParams(form, false);
    expect(params).toEqual({
      project_id: "p1",
      kind: "quick",
      title: "",
      description: "Fix the typo on the login page",
      owner_employee_id: "e1",
      run: {},
      confirm_secret: false,
    });
    expect(params).not.toHaveProperty("start");
  });

  it("takes files, a model and an effort along", () => {
    const params = quickCreateParams({ ...form, files: ["/u/a.png"], model: "opus", effort: "high" }, true);
    expect(params.files).toEqual(["/u/a.png"]);
    expect(params.run).toEqual({ model: "opus", effort: "high" });
    expect(params.confirm_secret).toBe(true);
  });

  it("needs a description and an employee", () => {
    expect(quickProblem({ description: " ", employeeId: "e1" })).toBe("Describe what needs doing.");
    expect(quickProblem({ description: "x", employeeId: null })).toBe("Choose who does it.");
    expect(quickProblem({ description: "x", employeeId: "e1" })).toBeNull();
  });

  it("starts with the employee last chosen for one, while still active", () => {
    const employees = [
      { id: "e1", status: "active" },
      { id: "e2", status: "paused" },
    ];
    expect(quickAssignee(employees, "e1")).toBe("e1");
    expect(quickAssignee(employees, "e2")).toBeNull();
    expect(quickAssignee(employees, "gone")).toBeNull();
    expect(quickAssignee(employees, null)).toBeNull();
  });
});

describe("creating a normal ticket", () => {
  it("never sends a kind", () => {
    const params = ticketCreateParams(
      {
        projectId: "p1",
        title: " Title ",
        description: "",
        priority: "normal",
        labels: [],
        deadlineMs: null,
        files: [],
        start: false,
        run: {},
        responsibleUserId: null,
        ownerEmployeeId: null,
      },
      false,
    );
    expect(params).not.toHaveProperty("kind");
    expect(params).toEqual({
      project_id: "p1",
      title: "Title",
      description: null,
      priority: "normal",
      labels: [],
      deadline_ms: null,
      start: false,
      run: {},
      confirm_secret: false,
    });
  });
});
