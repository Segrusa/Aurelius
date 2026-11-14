import { describe, expect, it } from "vitest";
import { actionsFor, chordOf, type KeyState, keysFor, SHORTCUTS, showChord } from "./keymap";

const press = (
  key: string,
  code = "",
  mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {},
) => ({ key, code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });
const base: KeyState = {
  stack: [
    { name: "Aurelius", modal: false },
    { name: "Workspace", modal: false },
  ],
  typing: false,
  onControl: false,
  kind: "electron",
};

describe("the table", () => {
  it("holds the desktop's shortcuts that do something here, none twice for one chord in one context", () => {
    expect(SHORTCUTS.length).toBeGreaterThanOrEqual(73);
    const seen = new Set<string>();
    for (const s of SHORTCUTS) {
      for (const k of s.keys) {
        const id = `${s.context}/${k}`;
        expect(seen.has(id), id).toBe(false);
        seen.add(id);
      }
    }
  });
});

describe("a key press", () => {
  it("becomes the table's notation", () => {
    expect(chordOf(press("k", "KeyK", { ctrlKey: true }), false)).toBe("secondary-k");
    expect(chordOf(press("k", "KeyK", { metaKey: true }), true)).toBe("secondary-k");
    expect(chordOf(press("k", "KeyK", { metaKey: true }), false)).toBe("k");
    expect(chordOf(press("ArrowDown", "ArrowDown"), false)).toBe("down");
    expect(chordOf(press("Enter", "Enter", { shiftKey: true }), false)).toBe("shift-enter");
    expect(chordOf(press("Shift", "ShiftLeft", { shiftKey: true }), false)).toBeNull();
  });

  it("works on an AZERTY keyboard: letters by layout, digits and punctuation by place", () => {
    // AZERTY: the key that types M sits on the Latin semicolon; 1 needs Shift, without it types &.
    expect(chordOf(press("m", "Semicolon", { ctrlKey: true }), false)).toBe("secondary-m");
    expect(chordOf(press("&", "Digit1", { ctrlKey: true }), false)).toBe("secondary-1");
    expect(chordOf(press("<", "Comma", { ctrlKey: true, shiftKey: true }), false)).toBe("secondary-shift-,");
  });
});

describe("what a chord means", () => {
  it("in the app and in a list", () => {
    expect(actionsFor("secondary-k", base)).toEqual(["OpenPalette"]);
    expect(actionsFor("j", base)).toEqual(["SelectNext"]);
    expect(actionsFor("down", base)).toEqual(["SelectNext"]);
  });

  it("no longer knows the keys of a hidden feature", () => {
    expect(actionsFor("secondary-shift-v", base)).toEqual([]);
    expect(actionsFor("secondary-0", base)).toEqual([]);
    expect(actionsFor("secondary-7", base)).toEqual(["GoTasks"]);
  });

  it("leaves typing alone, except what has a modifier", () => {
    const typing = { ...base, typing: true };
    expect(actionsFor("j", typing)).toEqual([]);
    expect(actionsFor("1", typing)).toEqual([]);
    expect(actionsFor("enter", typing)).toEqual([]);
    expect(actionsFor("secondary-k", typing)).toEqual(["OpenPalette"]);
    expect(actionsFor("escape", typing)).toEqual(["Back"]);
  });

  it("leaves Enter on a focused button to the button", () => {
    expect(actionsFor("enter", { ...base, onControl: true })).toEqual([]);
    expect(actionsFor("enter", base)).toEqual(["OpenSelected"]);
  });

  it("takes the innermost context first, and a modal one shuts out the rest", () => {
    const inbox = { ...base, stack: [...base.stack, { name: "Inbox", modal: false }] };
    expect(actionsFor("enter", inbox)).toEqual(["AllowOnce", "OpenSelected"]);
    const dialog = { ...base, stack: [...base.stack, { name: "AureliusDialog", modal: true }] };
    expect(actionsFor("secondary-k", dialog)).toEqual([]);
    expect(actionsFor("secondary-enter", dialog)).toEqual(["ConfirmPrimary"]);
  });

  it("starts and cycles Give work's choices on New session, also while typing in its composer", () => {
    const session = { ...base, typing: true, stack: [...base.stack, { name: "NewSession", modal: false }] };
    expect(actionsFor("secondary-enter", session)[0]).toBe("StartNewSession");
    expect(actionsFor("alt-m", session)[0]).toBe("CycleTaskManager");
    expect(actionsFor("alt-o", session)[0]).toBe("CycleTaskMode");
    expect(actionsFor("alt-l", session)[0]).toBe("CycleTaskModel");
    expect(actionsFor("alt-e", session)[0]).toBe("CycleTaskEffort");
    expect(actionsFor("alt-j", session)).toContain("CycleGiveProject");
  });

  it("gets another key where a browser keeps it", () => {
    expect(keysFor(["secondary-t"], "web")).toEqual(["alt-t"]);
    expect(keysFor(["secondary-q"], "web")).toEqual([]);
    expect(keysFor(["secondary-t"], "electron")).toEqual(["secondary-t"]);
    const web = { ...base, kind: "web" as const };
    expect(actionsFor("alt-t", web)).toEqual(["MakeTicket"]);
    expect(actionsFor("secondary-t", web)).toEqual([]);
  });
});

describe("a chord on screen", () => {
  it("follows the platform", () => {
    expect(showChord("secondary-shift-n", false)).toBe("Ctrl ⇧ N");
    expect(showChord("secondary-enter", true)).toBe("⌘↵");
  });
});
