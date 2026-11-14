import type { Receipt } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  createdTicketId,
  createdTicketPlace,
  cycle,
  DIALOG_TOP,
  defaultAssignee,
  giveWorkParams,
  parseDeadline,
  parseLabels,
  rootsOf,
  taskRun,
  titleOfGoal,
  topCentered,
} from "./dialog-logic";

describe("a deadline", () => {
  it("is empty, a date (the end of that day) or a date and time, in local time", () => {
    expect(parseDeadline("  ", 0)).toBeNull();
    expect(parseDeadline("2026-11-04", 0)).toBe(Date.UTC(2026, 10, 4, 23, 59));
    expect(parseDeadline("2026-11-04 17:00", 0)).toBe(Date.UTC(2026, 10, 4, 17, 0));
    expect(parseDeadline("2026-11-04T17:00", 60)).toBe(Date.UTC(2026, 10, 4, 16, 0));
  });

  it("refuses what is no date", () => {
    for (const bad of [
      "tomorrow",
      "2026-13-01",
      "2026-02-30",
      "2026-11-04 25:00",
      "2026-11-04 17",
      "2026-11",
      "1969-01-01",
      "2026-11-04 17:00 x",
    ]) {
      expect(() => parseDeadline(bad, 0), bad).toThrow(/Write the date/);
    }
  });
});

describe("the rest", () => {
  it("splits labels, without doubles", () => {
    expect(parseLabels(" backend, ui ,,backend")).toEqual(["backend", "ui"]);
  });

  it("makes a title of a goal, as the domain crate does", () => {
    expect(titleOfGoal("  Compare hosting\nwith details")).toBe("Compare hosting");
    const title = titleOfGoal("x".repeat(80));
    expect([...title]).toHaveLength(61);
    expect(title.endsWith("…")).toBe(true);
    expect(titleOfGoal("x".repeat(60))).toBe("x".repeat(60));
  });

  it("gives work to the top of the org chart", () => {
    const ceo = { id: "ceo", status: "active", reports_to: null };
    const cto = { id: "cto", status: "active", reports_to: "ceo" };
    const orphan = { id: "orphan", status: "active", reports_to: "gone" };
    const gone = { id: "gone", status: "archived", reports_to: null };
    expect(rootsOf([ceo, cto]).map((e) => e.id)).toEqual(["ceo"]);
    expect(rootsOf([ceo, orphan, gone]).map((e) => e.id)).toEqual(["ceo", "orphan"]);
    expect(defaultAssignee([], null)).toBeNull();
    expect(defaultAssignee([ceo, cto], "cto")).toBe("ceo");
    expect(defaultAssignee([ceo, orphan, cto], "cto")).toBe("cto");
    expect(defaultAssignee([ceo, orphan, gone], "gone")).toBe("ceo");
  });

  it("cycles around", () => {
    expect(cycle(["a", "b"], "a")).toBe("b");
    expect(cycle(["a", "b"], "b")).toBe("a");
    expect(cycle(["a", "b"], "c")).toBe("a");
  });
});

describe("a dialog pinned near the top", () => {
  it("keeps its top and is centred by its width, never off the left edge", () => {
    expect(topCentered(640)).toEqual({ top: DIALOG_TOP, start: "max(0px, calc((100vw - 640px) / 2))" });
  });
});

describe("the ticket a create made", () => {
  const receipt = (result: Receipt["result"]): Receipt => ({
    committed_seq: 1,
    idempotency_key: "k",
    replayed: false,
    result,
  });

  it("is the id in the answer to ticket.create, so the dialog can open its chat", () => {
    expect(
      createdTicketId(receipt({ kind: "command", value: { type: "ticket_created", number: 58, ticket_id: "t-58" } })),
    ).toBe("t-58");
  });

  it("is none for any other answer", () => {
    expect(
      createdTicketId(
        receipt({ kind: "command", value: { type: "ticket_changed", column: "doing", ticket_id: "t-1", version: 2 } }),
      ),
    ).toBeNull();
  });

  it("opens on its channel in Chat, or on the Board when there is no channel", () => {
    expect(createdTicketPlace("t-58", "c-58")).toEqual({ to: "/chat", search: { channel: "c-58" } });
    expect(createdTicketPlace("t-58", null)).toEqual({ to: "/board", search: { ticket: "t-58" } });
  });
});

describe("the run a dialog sends", () => {
  it("leaves out every choice still on Default, so the owner's own settings apply", () => {
    expect(taskRun("default", "default")).toEqual({});
    expect(taskRun("default", "default", "default")).toEqual({});
  });

  it("names the mode, model and effort that were chosen", () => {
    expect(taskRun("plan", "claude-opus-5-5", "high")).toEqual({
      mode: "plan",
      model: "claude-opus-5-5",
      effort: "high",
    });
    expect(taskRun("auto", "default")).toEqual({ mode: "auto" });
  });
});

describe("the ticket Give work makes", () => {
  const form = {
    projectId: "p-1",
    goal: "  Compare three hosting options\nwith prices  ",
    employeeId: "e-1",
    run: { mode: "plan" as const, effort: "high" },
    files: [] as string[],
    responsibleUserId: null as string | null,
  };

  it("is started at once for the chosen employee, titled by its first line", () => {
    expect(giveWorkParams(form, false)).toEqual({
      project_id: "p-1",
      title: "Compare three hosting options",
      description: "Compare three hosting options\nwith prices",
      owner_employee_id: "e-1",
      start: true,
      run: { mode: "plan", effort: "high" },
      confirm_secret: false,
    });
  });

  it("adds files and the responsible board member only when there are any", () => {
    expect(giveWorkParams({ ...form, files: ["/tmp/a.png"], responsibleUserId: "u-1" }, true)).toEqual({
      project_id: "p-1",
      title: "Compare three hosting options",
      description: "Compare three hosting options\nwith prices",
      owner_employee_id: "e-1",
      start: true,
      run: { mode: "plan", effort: "high" },
      files: ["/tmp/a.png"],
      confirm_secret: true,
      responsible_user_id: "u-1",
    });
  });
});
