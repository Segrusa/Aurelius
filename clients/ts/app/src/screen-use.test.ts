import { describe, expect, it } from "vitest";
import { hidden, shown } from "./hidden";
import { noteScreen, parseScreenUse, type ScreenUse, screenKey, screenUseJson } from "./screen-use";

describe("the screen counter", () => {
  it("counts an open, not the screen already showing", () => {
    const use: ScreenUse = {};
    expect(noteScreen(use, null, "inbox", 1)).toBe(true);
    expect(noteScreen(use, "inbox", "inbox", 2)).toBe(false);
    expect(noteScreen(use, "board", "inbox", 3)).toBe(true);
    expect(use.inbox).toEqual({ opens: 2, first_ms: 1, last_ms: 3 });
  });

  it("stops at its limits", () => {
    const use: ScreenUse = {};
    for (let i = 0; i < 64; i++) noteScreen(use, null, `s${i}`, 1);
    expect(noteScreen(use, null, "one-too-many", 1)).toBe(false);
    expect(noteScreen(use, null, "s1", 2)).toBe(true);
    expect(noteScreen({}, null, "x".repeat(49), 1)).toBe(false);
    expect(noteScreen({}, null, "", 1)).toBe(false);
  });

  it("keeps the desktop's file shape and survives damage", () => {
    const use: ScreenUse = { board: { opens: 3, first_ms: 1, last_ms: 2 } };
    expect(screenUseJson(use)).toBe('{"screens":{"board":{"opens":3,"first_ms":1,"last_ms":2}}}');
    expect(parseScreenUse(screenUseJson(use))).toEqual(use);
    expect(parseScreenUse("not json")).toEqual({});
    expect(parseScreenUse('{"screens":{"a":{"opens":"x"}}}')).toEqual({});
    expect(parseScreenUse(null)).toEqual({});
  });

  it("names a screen without ids", () => {
    expect(screenKey("/inbox")).toBe("inbox");
    expect(screenKey("/settings/app")).toBe("settings.app");
  });
});

describe("hidden features", () => {
  it("are the desktop's", () => {
    expect(hidden("ChatAttachments")).toBe(true);
  });

  it("are the simplification's", () => {
    for (const feature of ["ActivityInSidebar", "Vault", "MemoryProposals", "SequentialTasks"] as const) {
      expect(hidden(feature)).toBe(true);
    }
  });

  it("leave only what is shown of a list", () => {
    expect(shown([{ key: "a" }, { key: "b", feature: "Vault" as const }]).map((i) => i.key)).toEqual(["a"]);
  });
});
