import type { ActivityView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { answerParts, stepsLabel } from "./answer-steps-logic";

const step = (tool: string, offset: number, input: unknown = {}, ok: boolean | null = true): ActivityView => ({
  call_id: `${tool}-${offset}-${JSON.stringify(input)}`,
  tool,
  input,
  offset,
  ok,
  truncated: false,
});

describe("answerParts", () => {
  it("an answer without steps is one text part", () => {
    expect(answerParts("Hello.", [])).toEqual([{ kind: "text", text: "Hello." }]);
  });

  it("puts the steps between the text written before and after them", () => {
    const text = "First I look.\n\nNow I edit.";
    const parts = answerParts(text, [step("Bash", 13), step("Read", 13, { file_path: "a.ts" })]);
    expect(parts.map((p) => p.kind)).toEqual(["text", "steps", "text"]);
    expect(parts[0]).toEqual({ kind: "text", text: "First I look." });
    expect(parts[2]).toEqual({ kind: "text", text: "Now I edit." });
    const group = parts[1];
    expect(group?.kind === "steps" ? group.steps.length : 0).toBe(2);
  });

  it("steps before any text come first, and steps at the end come last", () => {
    const parts = answerParts("Done.", [step("Bash", 0), step("Bash", 5)]);
    expect(parts.map((p) => p.kind)).toEqual(["steps", "text", "steps"]);
  });

  it("reads the offset as UTF-8 bytes, not UTF-16 units", () => {
    // "é" is two bytes and "🙂" four: byte 11 is just after the full stop (UTF-16 index 8).
    const text = "é 🙂 ok.Next";
    const parts = answerParts(text, [step("Bash", 11)]);
    expect(parts).toEqual([
      { kind: "text", text: "é 🙂 ok." },
      expect.objectContaining({ kind: "steps" }),
      { kind: "text", text: "Next" },
    ]);
  });

  it("an offset past the end of the text puts the steps after it", () => {
    const parts = answerParts("Short.", [step("Bash", 999)]);
    expect(parts.map((p) => p.kind)).toEqual(["text", "steps"]);
  });

  it("keeps steps in the order they ran, even when an offset goes back", () => {
    const parts = answerParts("Ab", [step("Bash", 2), step("Read", 1)]);
    expect(parts.map((p) => p.kind)).toEqual(["text", "steps"]);
  });
});

describe("stepsLabel", () => {
  it("says what was done, like Claude does", () => {
    const steps = [
      step("Bash", 0, { command: "ls" }),
      step("Bash", 0, { command: "pwd" }),
      step("Read", 0, { file_path: "/x/employee-chat.tsx" }),
    ];
    expect(stepsLabel(steps)).toBe("Ran 2 commands, read employee-chat.tsx");
  });

  it("counts files when there is more than one", () => {
    const steps = [
      step("Read", 0, { file_path: "/a.ts" }),
      step("Read", 0, { file_path: "/b.ts" }),
      step("Edit", 0, { file_path: "/b.ts" }),
    ];
    expect(stepsLabel(steps)).toBe("Read 2 files, edited b.ts");
  });

  it("names searches, web use and other tools", () => {
    const steps = [
      step("Grep", 0, { pattern: "x" }),
      step("Glob", 0, { pattern: "*.ts" }),
      step("WebSearch", 0, { query: "q" }),
      step("mcp__aurelius__recall", 0),
    ];
    expect(stepsLabel(steps)).toBe("Searched 2 times, searched the web, used 1 tool");
  });

  it("says when steps failed or still run", () => {
    expect(stepsLabel([step("Bash", 0, {}, false), step("Bash", 0)])).toBe("Ran 2 commands · 1 failed");
    expect(stepsLabel([step("Bash", 0, {}, null)])).toBe("Running 1 command…");
  });
});
