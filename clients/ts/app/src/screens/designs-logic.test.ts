import type { DesignView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { designPlace, pagesOf } from "./designs-logic";

const design = (over: Partial<DesignView> = {}): DesignView => ({
  conversation_id: "c1",
  kind: "session",
  project_id: "p1",
  title: "Landing page",
  pages: [
    {
      file: { id: "f1", name: "landing.html", media_type: "text/html", size: 10 },
      message_id: "m1",
      at_ms: 100,
    },
  ],
  last_at_ms: 100,
  ...over,
});

describe("designPlace", () => {
  it("opens a session, a ticket by number and a task where they live", () => {
    expect(designPlace(design())).toEqual({ label: "Session", to: "/session/c1" });
    expect(designPlace(design({ kind: "ticket", ticket_number: 12 }))).toEqual({
      label: "Ticket #12",
      to: "/ticket/12",
    });
    expect(designPlace(design({ kind: "task", task_id: "t9" }))).toEqual({ label: "Task", to: "/tasks/t9" });
  });

  it("has no link where the app has no screen for the conversation", () => {
    expect(designPlace(design({ kind: "ticket" }))).toEqual({ label: "Ticket", to: null });
    expect(designPlace(design({ kind: "project" }))).toEqual({ label: "Project chat", to: null });
  });
});

describe("pagesOf", () => {
  it("gives the viewer the design's pages", () => {
    expect(pagesOf(design())).toEqual([
      {
        name: "landing.html",
        file: { id: "f1", name: "landing.html", media_type: "text/html", size: 10 },
        messageId: "m1",
        atMs: 100,
      },
    ]);
  });
});
