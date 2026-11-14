import { describe, expect, it } from "vitest";
import { offered } from "./hidden";

describe("rows that need a feature of the server", () => {
  const rows = [{ to: "/board" }, { to: "/history", server: "ticket_history" }];

  it("show only when the server has it", () => {
    expect(offered(rows, ["ticket_history"]).map((r) => r.to)).toEqual(["/board", "/history"]);
    expect(offered(rows, []).map((r) => r.to)).toEqual(["/board"]);
  });
});
