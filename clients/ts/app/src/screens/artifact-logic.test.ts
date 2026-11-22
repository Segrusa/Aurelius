import type { FileRef, MessageView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  artifactVersions,
  DIALOG_SELECTOR,
  htmlOf,
  isArtifact,
  OVERLAY_SELECTOR,
  type Overlay,
  overlaysOver,
  stageBounds,
  versionLabel,
  versionOf,
  versionsFor,
  viewportWidth,
} from "./artifact-logic";

const html = (id: string, name = "landing.html"): FileRef => ({ id, name, media_type: "text/html", size: 10 });

const msg = (id: string, ordinal: number, files: FileRef[], over: Partial<MessageView> = {}): MessageView => ({
  id,
  activity: [],
  created_at_ms: ordinal * 1000,
  files,
  ordinal,
  queued: false,
  role: "assistant",
  sources: [],
  status: "complete",
  text: "",
  uploads: [],
  ...over,
});

describe("isArtifact", () => {
  it("is an HTML file, with or without a charset", () => {
    expect(isArtifact({ media_type: "text/html" })).toBe(true);
    expect(isArtifact({ media_type: "text/html; charset=utf-8" })).toBe(true);
    expect(isArtifact({ media_type: "TEXT/HTML" })).toBe(true);
  });

  it("leaves every other file as it was", () => {
    for (const type of ["image/png", "image/svg+xml", "text/plain", "application/pdf", "text/markdown"]) {
      expect(isArtifact({ media_type: type })).toBe(false);
    }
  });
});

describe("artifactVersions", () => {
  it("numbers the same name v1..vN in message order and lists the newest first", () => {
    const messages = [msg("a", 1, [html("f1")]), msg("b", 2, []), msg("c", 3, [html("f2")]), msg("d", 4, [html("f3")])];
    const versions = artifactVersions(messages, "landing.html");
    expect(versions.map((v) => [v.version, v.file.id, v.messageId])).toEqual([
      [3, "f3", "d"],
      [2, "f2", "c"],
      [1, "f1", "a"],
    ]);
  });

  it("follows the ordinal, not the order the list arrived in", () => {
    const messages = [msg("late", 9, [html("f2")]), msg("early", 2, [html("f1")])];
    expect(artifactVersions(messages, "landing.html").map((v) => `v${v.version}:${v.file.id}`)).toEqual([
      "v2:f2",
      "v1:f1",
    ]);
  });

  it("counts only HTML files with exactly that name", () => {
    const messages = [
      msg("a", 1, [html("f1"), html("other", "pricing.html")]),
      msg("b", 2, [{ id: "png", name: "landing.html", media_type: "image/png", size: 1 }]),
      msg("c", 3, [html("f2", "Landing.html")]),
    ];
    expect(artifactVersions(messages, "landing.html").map((v) => v.file.id)).toEqual(["f1"]);
  });

  it("skips a deleted message", () => {
    const messages = [msg("a", 1, [html("f1")]), msg("b", 2, [html("f2")], { deleted_at_ms: 5 })];
    expect(artifactVersions(messages, "landing.html").map((v) => v.file.id)).toEqual(["f1"]);
  });

  it("keeps the time of the message each version came with", () => {
    const [newest] = artifactVersions([msg("a", 1, [html("f1")]), msg("b", 7, [html("f2")])], "landing.html");
    expect(newest?.atMs).toBe(7000);
  });
});

describe("versionsFor", () => {
  it("offers the conversation's versions of an opened file", () => {
    const messages = [msg("a", 1, [html("f1")]), msg("b", 2, [html("f2")])];
    expect(versionsFor(messages, html("f1"), 0).map((v) => v.version)).toEqual([2, 1]);
  });

  it("offers the file alone when its message is not in the list", () => {
    const versions = versionsFor([], html("lonely"), 42);
    expect(versions).toEqual([{ version: 1, file: html("lonely"), messageId: "", atMs: 42 }]);
  });
});

describe("versionOf", () => {
  const versions = artifactVersions([msg("a", 1, [html("f1")]), msg("b", 2, [html("f2")])], "landing.html");

  it("says which version a card is", () => {
    expect(versionOf(versions, "f1")).toBe("v1 of 2");
    expect(versionOf(versions, "f2")).toBe("v2 of 2");
  });

  it("says nothing for a single version or an unknown file", () => {
    expect(versionOf(versions.slice(0, 1), "f2")).toBeNull();
    expect(versionOf(versions, "nope")).toBeNull();
  });
});

describe("versionLabel", () => {
  it("shows the time today and the date before today", () => {
    const now = new Date(2026, 10, 22, 15, 0).getTime();
    const today = versionLabel(
      { version: 3, file: html("f"), messageId: "m", atMs: new Date(2026, 10, 22, 9, 5).getTime() },
      now,
    );
    const earlier = versionLabel(
      { version: 1, file: html("f"), messageId: "m", atMs: new Date(2026, 10, 20, 9, 5).getTime() },
      now,
    );
    expect(today).toMatch(/^v3 · 09.05$/);
    expect(earlier).toMatch(/^v1 · .*20.*, 09.05$/);
  });
});

describe("the frame", () => {
  const stage = { left: 40, top: 100, right: 1400, bottom: 700 };
  const window = { width: 1440, height: 900 };

  it("lies over the frame box where it is inside the stage", () => {
    const frame = { left: 330, top: 100, right: 720, bottom: 700 };
    expect(stageBounds(frame, stage, window)).toEqual({ x: 330, y: 100, width: 390, height: 600 });
  });

  it("is cut to the stage and to the window", () => {
    const wide = { left: 0, top: 100, right: 1600, bottom: 700 };
    expect(stageBounds(wide, stage, window)).toEqual({ x: 40, y: 100, width: 1360, height: 600 });
    const low = { left: 40, top: 600, right: 1400, bottom: 1200 };
    expect(stageBounds(low, { ...stage, bottom: 1200 }, window)).toEqual({
      x: 40,
      y: 600,
      width: 1360,
      height: 300,
    });
  });

  it("is hidden when nothing of it shows, or an open menu of the page lies over it", () => {
    const frame = { left: 330, top: 100, right: 720, bottom: 700 };
    expect(stageBounds({ left: 40, top: 900, right: 1400, bottom: 1500 }, stage, window)).toBeNull();
    expect(stageBounds({ left: 40, top: 100, right: 40, bottom: 700 }, stage, window)).toBeNull();
    const versionList = { left: 300, top: 80, right: 500, bottom: 220 };
    expect(stageBounds(frame, stage, window, [versionList])).toBeNull();
  });

  it("stays when a menu is open elsewhere", () => {
    const frame = { left: 330, top: 100, right: 720, bottom: 700 };
    const sidebarMenu = { left: 0, top: 0, right: 300, bottom: 90 };
    expect(stageBounds(frame, stage, window, [sidebarMenu])).toEqual({ x: 330, y: 100, width: 390, height: 600 });
  });

  it("steps aside for a toast, a confirm or another dialog that opened after it", () => {
    const frame = { left: 330, top: 100, right: 720, bottom: 700 };
    const over = (o: Partial<Overlay>): Overlay => ({
      rect: { left: 400, top: 300, right: 700, bottom: 400, width: 300, height: 100 },
      visible: true,
      isDialog: false,
      openBefore: false,
      holdsStage: false,
      ...o,
    });
    const toast = over({});
    const confirm = over({ isDialog: true });
    for (const shown of [toast, confirm]) expect(stageBounds(frame, stage, window, overlaysOver([shown]))).toBeNull();
  });

  it("stays for its own dialog, a dialog it opened from, and what is hidden or empty", () => {
    const frame = { left: 330, top: 100, right: 720, bottom: 700 };
    const all = { left: 0, top: 0, right: 1440, bottom: 900, width: 1440, height: 900 };
    const under: Overlay[] = [
      { rect: all, visible: true, isDialog: true, openBefore: true, holdsStage: true },
      { rect: all, visible: true, isDialog: true, openBefore: false, holdsStage: true },
      { rect: all, visible: true, isDialog: true, openBefore: true, holdsStage: false },
      { rect: all, visible: false, isDialog: false, openBefore: false, holdsStage: false },
      { rect: { ...all, right: 0, width: 0 }, visible: true, isDialog: false, openBefore: false, holdsStage: false },
    ];
    expect(overlaysOver(under)).toEqual([]);
    expect(stageBounds(frame, stage, window, overlaysOver(under))).toEqual({ x: 330, y: 100, width: 390, height: 600 });
    // A menu inside its own dialog (the version list) still counts.
    expect(overlaysOver([{ ...under[0], isDialog: false } as Overlay])).toHaveLength(1);
  });

  it("looks for menus, toasts and dialogs", () => {
    for (const part of ['[role="menu"]', '[role="listbox"]', "[data-toast-id]", "dialog[open]", '[role="alertdialog"]'])
      expect(OVERLAY_SELECTOR).toContain(part);
    expect(DIALOG_SELECTOR).toContain('[role="alertdialog"]');
  });

  it("has a width per viewport", () => {
    expect([viewportWidth("desktop"), viewportWidth("tablet"), viewportWidth("mobile")]).toEqual([1280, 768, 390]);
  });

  it("reads the file's bytes as UTF-8", () => {
    const text = "<p>Café ✓</p>";
    const base64 = btoa(String.fromCharCode(...new TextEncoder().encode(text)));
    expect(htmlOf(base64)).toBe(text);
  });
});
