import type { ActivityView, FileRef, MessageView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  applyEdit,
  baseName,
  DIALOG_SELECTOR,
  designPages,
  designsOf,
  designTicket,
  designToOpen,
  htmlOf,
  isArtifact,
  liveStep,
  OVERLAY_SELECTOR,
  type Overlay,
  overlaysOver,
  stageBounds,
  viewportWidth,
  withDrag,
  writtenContent,
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

describe("designPages", () => {
  it("is one page per name, the newest kept file of each, in the order the pages first came", () => {
    const messages = [msg("a", 1, [html("l1")]), msg("b", 2, [html("p1", "pricing.html")]), msg("c", 3, [html("l2")])];
    expect(designPages(messages).map((p) => [p.name, p.file.id, p.messageId])).toEqual([
      ["landing.html", "l2", "c"],
      ["pricing.html", "p1", "b"],
    ]);
  });

  it("follows the ordinal, not the order the list arrived in", () => {
    const messages = [msg("late", 9, [html("f2")]), msg("early", 2, [html("f1")])];
    expect(designPages(messages).map((p) => p.file.id)).toEqual(["f2"]);
  });

  it("takes only HTML files, and skips a deleted message", () => {
    const messages = [
      msg("a", 1, [html("f1")]),
      msg("b", 2, [{ id: "png", name: "landing.html", media_type: "image/png", size: 1 }]),
      msg("c", 3, [html("f2")], { deleted_at_ms: 5 }),
    ];
    expect(designPages(messages).map((p) => p.file.id)).toEqual(["f1"]);
  });

  it("adds an opened file whose message is not in the list as a page of its own", () => {
    const pages = designPages([msg("a", 1, [html("f1")])], html("lonely", "about.html"), 42);
    expect(pages.map((p) => [p.name, p.file.id, p.atMs])).toEqual([
      ["landing.html", "f1", 1000],
      ["about.html", "lonely", 42],
    ]);
    // An older copy of a page that is there opens that page, at its newest.
    expect(
      designPages([msg("a", 1, [html("f1")]), msg("b", 2, [html("f2")])], html("f1")).map((p) => p.file.id),
    ).toEqual(["f2"]);
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

describe("designs written live", () => {
  const step = (
    call_id: string,
    tool: string,
    input: object,
    ok: boolean | null = true,
    parent: string | null = null,
  ) => ({ call_id, tool, input, ok, output: null, offset: 0, parent_call_id: parent }) as unknown as ActivityView;

  it("are an answer's own HTML writes, newest write per file, with the edits after it", () => {
    const designs = designsOf(
      [
        step("w1", "Write", { file_path: "/p/a.html", content: "<p>1</p>" }),
        step("e0", "Edit", { file_path: "/p/b.html", old_string: "x", new_string: "y" }),
        step("n1", "Write", { file_path: "/p/notes.md", content: "#" }),
        step("s1", "Write", { file_path: "/p/sub.html", content: "<p>" }, true, "agent"),
        step("e1", "Edit", { file_path: "/p/a.html", old_string: "1", new_string: "2" }),
        step("e2", "Edit", { file_path: "/p/a.html", old_string: "2", new_string: "3" }, false),
        step("w2", "Write", { file_path: "C:\\p\\B.HTM", content: "<b>" }, null),
      ],
      null,
    );
    expect(designs).toEqual([
      { path: "/p/a.html", name: "a.html", callId: "w1", writing: false, draft: null, edits: ["e1"] },
      { path: "C:\\p\\B.HTM", name: "B.HTM", callId: "w2", writing: true, draft: null, edits: [] },
    ]);
  });

  it("show the draft until its call is whole", () => {
    const draft = { call_id: "w1", path: "/p/a.html", content: "<p>Hal" };
    expect(designsOf([], draft)).toEqual([
      { path: "/p/a.html", name: "a.html", callId: "w1", writing: true, draft: "<p>Hal", edits: [] },
    ]);
    const whole = [step("w1", "Write", { file_path: "/p/a.html", content: "<p>Hallo</p>" }, null)];
    expect(designsOf(whole, draft)[0]?.draft).toBeNull();
    expect(designsOf([], { ...draft, path: "/p/a.md" })).toEqual([]);
  });

  it("apply edits as the tools do, and refuse one that does not fit", () => {
    expect(applyEdit("<p>a a</p>", { old_string: "a", new_string: "$&b" })).toBe("<p>$&b a</p>");
    expect(applyEdit("<p>a a</p>", { old_string: "a", new_string: "b", replace_all: true })).toBe("<p>b b</p>");
    expect(
      applyEdit("<p>a</p>", {
        edits: [
          { old_string: "a", new_string: "b" },
          { old_string: "b", new_string: "c" },
        ],
      }),
    ).toBe("<p>c</p>");
    expect(applyEdit("<p>a</p>", { old_string: "z", new_string: "b" })).toBeNull();
    expect(
      applyEdit("<p>a</p>", {
        edits: [
          { old_string: "a", new_string: "b" },
          { old_string: "a", new_string: "c" },
        ],
      }),
    ).toBeNull();
    expect(writtenContent({ file_path: "/p/a.html", content: "<p>" })).toBe("<p>");
    expect(baseName("/p/x/y.html")).toBe("y.html");
  });

  it("open by themselves only while a draft streams, once per write", () => {
    const draft = { call_id: "w2", path: "/p/b.html", content: "<p>" };
    // A loaded chat: designs without a draft, also an unfinished one, never open by themselves.
    const history = [step("w1", "Write", { file_path: "/p/a.html", content: "<p>" }, null)];
    expect(designToOpen(designsOf(history, null), new Set())).toBeNull();
    expect(designToOpen(designsOf(history, draft), new Set())?.callId).toBe("w2");
    // Closed by the user (already opened once): it stays closed until a new write.
    expect(designToOpen(designsOf(history, draft), new Set(["w2"]))).toBeNull();
    expect(designToOpen(designsOf([], { ...draft, call_id: "w3" }), new Set(["w2"]))?.callId).toBe("w3");
  });
});

describe("withDrag", () => {
  it("tells when a drag starts and ends, and still calls the region's own handlers", () => {
    const calls: string[] = [];
    const props = {
      _size: 400,
      _onResizeStart: () => calls.push("start"),
      _onResizeMove: (d: number) => calls.push(`move ${d}`),
      _onResizeEnd: () => calls.push("end"),
      _onResizeCancel: () => calls.push("cancel"),
    };
    const wrapped = withDrag(props, (on) => calls.push(on ? "drag on" : "drag off"));
    wrapped._onResizeStart();
    wrapped._onResizeMove(12);
    wrapped._onResizeEnd();
    wrapped._onResizeStart();
    wrapped._onResizeCancel?.();
    expect(calls).toEqual(["drag on", "start", "move 12", "drag off", "end", "drag on", "start", "drag off", "cancel"]);
    expect(wrapped._size).toBe(400);
  });

  it("ends a cancelled drag also for a region without its own cancel", () => {
    const seen: boolean[] = [];
    const wrapped = withDrag(
      { _onResizeStart: () => undefined, _onResizeEnd: () => undefined } as {
        _onResizeStart: () => void;
        _onResizeEnd: () => void;
        _onResizeCancel?: () => void;
      },
      (on) => seen.push(on),
    );
    wrapped._onResizeStart();
    wrapped._onResizeCancel?.();
    expect(seen).toEqual([true, false]);
  });
});

describe("liveStep", () => {
  const a = "<h1>A</h1>";
  const ab = "<h1>A</h1><p>B</p>";
  it("loads the first draft, then grows it in the same view while it is written", () => {
    expect(liveStep(undefined, undefined, true)).toBe("none");
    expect(liveStep(undefined, a, true)).toBe("serve");
    expect(liveStep(a, a, true)).toBe("none");
    expect(liveStep(a, ab, true)).toBe("update");
  });
  it("loads a finished design once more only when it has scripts to run", () => {
    expect(liveStep(a, ab, false)).toBe("update");
    expect(liveStep(a, `${ab}<script>go()</script>`, false)).toBe("serve");
    expect(liveStep(a, `${ab}<SCRIPT type="module">`, false)).toBe("serve");
    expect(liveStep(a, `${ab}<scripts-list>`, false)).toBe("update");
  });
});

describe("designTicket", () => {
  it("asks to build the page, names its path and says the design is attached", () => {
    expect(designTicket("design/pricing.html")).toEqual({
      title: "Build design/pricing.html",
      description:
        "Build this page as designed in design/pricing.html. The design is attached: follow its layout, colours and texts.",
    });
  });
});
