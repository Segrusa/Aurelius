import type { ApprovalView, MessageView, NotificationView, TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  age,
  approvalAction,
  approvalSentence,
  buildInbox,
  filterInbox,
  isWaiting,
  needsYouCount,
  newNotes,
  notificationHeading,
  notificationPlace,
  openQuestion,
  waitingLabel,
} from "./inbox-logic";

const approval = (over: Partial<ApprovalView> = {}) =>
  ({
    id: "a",
    requester: "Flux",
    tool: "claude.Bash",
    arguments: { command: "ls\nrm -rf x" },
    status: "pending",
    requested_at_ms: 10,
    ...over,
  }) as ApprovalView;
const note = (over: Partial<NotificationView> = {}) =>
  ({
    id: "n",
    title: "T",
    subject: "S",
    read: false,
    created_at_ms: 5,
    severity: "info",
    target_type: "x",
    ...over,
  }) as NotificationView;
const task = (over: Partial<TaskView> = {}) =>
  ({ id: "t", title: "Work", status: "running", updated_at_ms: 1, ...over }) as TaskView;
const empty = { approvals: [], proposals: [], guidelineProposals: [], tasks: [], notifications: [] };

describe("approvals", () => {
  it("say what they would do, without Claude Code's prefix", () => {
    expect(approvalSentence(approval())).toBe("Flux wants to run a command");
    expect(approvalSentence(approval({ tool: "Write" }))).toBe("Flux wants to write a file");
    expect(approvalSentence(approval({ tool: "Odd" }))).toBe("Flux wants to use Odd");
    expect(approvalAction(approval()).body).toBe("ls\nrm -rf x");
    expect(approvalAction(approval({ tool: "Odd", arguments: { a: 1 } })).body).toContain('"a": 1');
  });

  it("list only while pending, with the first line of the command", () => {
    const groups = buildInbox({ ...empty, approvals: [approval(), approval({ id: "b", status: "approved" })] });
    expect(groups[0]?.rows.map((r) => r.id)).toEqual(["approval:a"]);
    expect(groups[0]?.rows[0]?.meta).toBe("Flux · ls");
  });
});

describe("a plan to review", () => {
  const waiting = task({ id: "t", ticket_id: "k", status: "waiting", waiting_reason: "plan_review" });

  it("is one decision, though the task waits and a notification says so", () => {
    const groups = buildInbox({
      ...empty,
      tasks: [waiting],
      notifications: [note({ reason: "plan_review", ticket_id: "k" })],
    });
    expect(groups.flatMap((g) => g.rows)).toHaveLength(1);
  });

  it("still shows when only the task says so, in words", () => {
    const rows = buildInbox({ ...empty, tasks: [waiting] }).flatMap((g) => g.rows);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.meta).toContain("plan to review");
    expect(waitingLabel("something_new")).toBe("something new");
  });
});

describe("tasks", () => {
  it("wait when waiting, or queued with a reason", () => {
    expect(isWaiting(task({ status: "waiting" }))).toBe(true);
    expect(isWaiting(task({ status: "queued", waiting_reason: "login" }))).toBe(true);
    expect(isWaiting(task({ status: "queued" }))).toBe(false);
    expect(isWaiting(task())).toBe(false);
  });
});

describe("notifications", () => {
  it("follow who reads them, and a reason this client does not know is the server's sentence", () => {
    expect(notificationHeading(note({ reason: "review", needs_you: true }))).toBe("Ready for your review");
    expect(notificationHeading(note({ reason: "review" }))).toBe("Ready for review");
    expect(notificationHeading(note({ reason: "from_a_newer_server", title: "Hello" }))).toBe("Hello");
  });

  it("say where they are", () => {
    expect(notificationPlace(note({ ticket_number: 42, subject: "Title" }))).toBe("#42 Title");
    expect(notificationPlace(note({ ticket_number: 42, subject: "" }))).toBe("#42");
    expect(notificationPlace(note({ subject: "general" }))).toBe("# general");
    expect(notificationPlace(note({ subject: "#general" }))).toBe("#general");
    expect(notificationPlace(note({ subject: "" }))).toBe("Update");
    expect(notificationPlace(note({ ticket_number: 1, subject: "#1 Login page" }))).toBe("#1 Login page");
    expect(notificationPlace(note({ ticket_number: 1, subject: "#10 Other" }))).toBe("#1 #10 Other");
  });

  it("group into reviews and updates; the newest decision comes first", () => {
    const groups = buildInbox({
      ...empty,
      approvals: [approval({ requested_at_ms: 1 }), approval({ id: "b", requested_at_ms: 2 })],
      notifications: [note({ id: "r", reason: "review", needs_you: true }), note({ id: "u", reason: "mention" })],
    });
    expect(groups.map((g) => g.key)).toEqual(["decide", "review", "updates"]);
    expect(groups[0]?.rows[0]?.id).toBe("approval:b");
    expect(needsYouCount(groups)).toBe(3);
  });
});

describe("filters", () => {
  const groups = buildInbox({
    ...empty,
    approvals: [approval()],
    notifications: [
      note({ id: "r", reason: "review" }),
      note({ id: "m", reason: "mention" }),
      note({ id: "x", reason: "failed" }),
    ],
  });

  it("keep what they name", () => {
    expect(filterInbox(groups, "all")).toBe(groups);
    expect(filterInbox(groups, "decisions").map((g) => g.key)).toEqual(["decide"]);
    expect(filterInbox(groups, "reviews").map((g) => g.key)).toEqual(["review"]);
    expect(filterInbox(groups, "mentions").flatMap((g) => g.rows.map((r) => r.id))).toEqual(["notification:m"]);
  });

  it("do not count a read review or someone else's", () => {
    const list = buildInbox({
      ...empty,
      notifications: [
        note({ id: "a", reason: "review", needs_you: true, read: true }),
        note({ id: "b", reason: "review" }),
      ],
    });
    expect(needsYouCount(list)).toBe(0);
  });
});

describe("age", () => {
  it("is short", () => {
    expect(age(0, 10_000)).toBe("now");
    expect(age(0, 120_000)).toBe("2m");
    expect(age(0, 8 * 3_600_000)).toBe("8h");
    expect(age(0, 49 * 3_600_000)).toBe("2d");
    expect(age(100_000, 0)).toBe("now");
  });
});

describe("newNotes", () => {
  const note = (id: string, at: number, read = false) =>
    ({ id, created_at_ms: at, read, severity: "info", subject: "", target_type: "x", title: id }) as NotificationView;
  it("says nothing on the first list", () => {
    expect(newNotes(null, [note("a", 1)])).toEqual([]);
  });
  it("names new rows and rows with a new event, never read ones", () => {
    const after = [note("a", 1), note("b", 2), note("c", 5), note("d", 3, true)];
    expect(newNotes([note("a", 1), note("c", 4)], after).map((n) => n.id)).toEqual(["b", "c"]);
  });
});

describe("one question, one row", () => {
  const empty = { approvals: [], proposals: [], guidelineProposals: [] };
  const waiting = {
    id: "w",
    title: "T",
    status: "waiting",
    waiting_reason: "input",
    ticket_id: "t1",
    updated_at_ms: 1,
  } as TaskView;
  const mention = (ticket: string) =>
    ({
      id: `n-${ticket}`,
      reason: "mention",
      ticket_id: ticket,
      title: "Pixel mentioned you",
      subject: "",
      created_at_ms: 1,
    }) as NotificationView;

  it("leaves out a mention on a ticket that already waits for an answer", () => {
    const groups = buildInbox({ ...empty, tasks: [waiting], notifications: [mention("t1"), mention("t2")] });
    expect(groups.map((g) => [g.key, g.rows.length])).toEqual([
      ["decide", 1],
      ["updates", 1],
    ]);
    expect(groups[1]?.rows[0]?.id).toBe("notification:n-t2");
  });

  it("finds the latest post that still waits", () => {
    const post = (id: string, awaiting: boolean, deleted?: number) =>
      ({ id, awaiting_user: awaiting, deleted_at_ms: deleted }) as MessageView;
    expect(openQuestion([post("a", true), post("b", true), post("c", false)])?.id).toBe("b");
    expect(openQuestion([post("a", true, 5)])).toBeNull();
    expect(openQuestion([])).toBeNull();
  });
});
