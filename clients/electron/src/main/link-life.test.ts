import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { appendLinkLog, linkLine, PageWatch } from "./link-life";

const main = { isMainFrame: true, isSameDocument: false };

/** Plays window events against a watch, as index.ts wires them; true when the link was closed. */
function closes(events: ("start" | "start-in-page" | "start-subframe" | "commit")[]): boolean {
  const watch = new PageWatch();
  for (const event of events) {
    if (event === "start") watch.started(main);
    else if (event === "start-in-page") watch.started({ isMainFrame: true, isSameDocument: true });
    else if (event === "start-subframe") watch.started({ isMainFrame: false, isSameDocument: false });
    else if (watch.committed()) return true;
  }
  return false;
}

describe("PageWatch", () => {
  it("keeps the link when a page change is blocked: a start without a commit", () => {
    expect(closes(["start"])).toBe(false);
  });

  it("closes the link when a page change goes through, as a reload does", () => {
    expect(closes(["start", "commit"])).toBe(true);
    expect(closes(["start", "start", "commit"])).toBe(true);
  });

  it("ignores a late commit of the page that opened the link", () => {
    expect(closes(["commit"])).toBe(false);
  });

  it("ignores in-page changes and frames inside the page", () => {
    expect(closes(["start-in-page", "start-subframe", "commit"])).toBe(false);
  });
});

describe("the link log", () => {
  const dir = mkdtempSync(join(tmpdir(), "aurelius-link-log-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));

  it("writes one line per event, with the reason and how long the link was open", () => {
    const line = linkLine(new Date(Date.UTC(2026, 10, 20, 13, 30, 20)), 7, "closed", {
      reason: "page changed",
      open_ms: 81234,
    });
    expect(line).toBe('2026-11-20T13:30:20.000Z link=7 closed reason="page changed" open_ms=81234');
  });

  it("moves a full log aside and starts a new one", () => {
    const file = join(dir, "links.log");
    appendLinkLog(file, "first", 10);
    appendLinkLog(file, "second line", 10);
    appendLinkLog(file, "third", 10);
    expect(readFileSync(`${file}.1`, "utf8")).toBe("first\nsecond line\n");
    expect(readFileSync(file, "utf8")).toBe("third\n");
  });

  it("never throws, also where it cannot write", () => {
    expect(() => appendLinkLog(join(dir, "links.log", "not-a-folder", "x.log"), "line")).not.toThrow();
  });
});
