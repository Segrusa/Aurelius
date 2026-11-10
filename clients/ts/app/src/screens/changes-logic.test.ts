import type { GitBranch } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  ago,
  changesLine,
  changesSearch,
  counts,
  parseDiff,
  splitPath,
  statusOf,
  ticketChanges,
  trackLine,
} from "./changes-logic";

const DIFF = `diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    old();
+    new();
+    more();
 }
\\ No newline at end of file
diff --git a/old name.rs b/new name.rs
similarity index 90%
rename from old name.rs
rename to new name.rs
diff --git a/logo.png b/logo.png
new file mode 100644
Binary files /dev/null and b/logo.png differ
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
`;

describe("a diff", () => {
  const files = parseDiff(DIFF);

  it("is one entry per file, with what happened to it", () => {
    expect(files.map((f) => [f.path, f.status, f.binary])).toEqual([
      ["src/main.rs", "modified", false],
      ["new name.rs", "renamed", false],
      ["logo.png", "added", true],
      ["gone.txt", "deleted", false],
    ]);
    expect(files[1]?.from).toBe("old name.rs");
  });

  it("numbers its lines on both sides and counts them", () => {
    const main = files[0];
    expect([main?.added, main?.removed]).toEqual([2, 1]);
    expect(main?.lines.map((l) => [l.kind, l.before, l.after])).toEqual([
      ["hunk", undefined, undefined],
      ["ctx", 1, 1],
      ["del", 2, undefined],
      ["add", undefined, 2],
      ["add", undefined, 3],
      ["ctx", 3, 4],
      ["note", undefined, undefined],
    ]);
    expect(main?.lines[3]?.text).toBe("    new();");
  });

  it("of nothing is nothing, and a line that only looks like a header inside a change is a change", () => {
    expect(parseDiff("")).toEqual([]);
    const tricky = parseDiff("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n--- not a header\n+++ nor this\n");
    expect(tricky[0]?.path).toBe("x");
    expect(tricky[0]?.lines.map((l) => l.kind)).toEqual(["hunk", "del", "add"]);
  });
});

describe("what the screen keeps in the URL", () => {
  it("is only what it knows", () => {
    expect(changesSearch({ view: "commits", rev: "feat/x", commit: "1a19f678", path: "src/a.rs" })).toEqual({
      view: "commits",
      rev: "feat/x",
      commit: "1a19f678",
      path: "src/a.rs",
    });
    expect(changesSearch({ view: "changes", commit: "not-a-commit", rev: 3 })).toEqual({});
    expect(changesSearch({ view: "elsewhere" })).toEqual({});
    // A base only narrows a branch that is named.
    expect(changesSearch({ view: "commits", rev: "ticket/1-x", base: "main" })).toEqual({
      view: "commits",
      rev: "ticket/1-x",
      base: "main",
    });
    expect(changesSearch({ view: "commits", base: "main" })).toEqual({ view: "commits" });
  });

  it("opens a ticket's branch against the one it was made from", () => {
    expect(ticketChanges({ branch: "ticket/1-x", base_branch: "main" })).toEqual({
      view: "commits",
      rev: "ticket/1-x",
      base: "main",
    });
    expect(ticketChanges({ branch: "ticket/1-x" })).toEqual({ view: "commits", rev: "ticket/1-x" });
    expect(ticketChanges({})).toBeNull();
  });
});

describe("words for changes and branches", () => {
  it("names a status, also one of a newer server", () => {
    expect(statusOf("untracked")).toEqual({ letter: "N", label: "New, not in git yet" });
    expect(statusOf("submodule")).toEqual({ letter: "•", label: "Changed" });
  });

  it("counts lines and files", () => {
    expect(counts({ added: 12, removed: 3 })).toBe("+12 −3");
    expect(counts({ added: 0, removed: 4 })).toBe("−4");
    expect(counts({})).toBe("");
    expect(splitPath("src/app/main.rs")).toEqual({ dir: "src/app/", name: "main.rs" });
    expect(splitPath("README.md")).toEqual({ dir: "", name: "README.md" });
    const change = (staged: boolean) => ({ path: "a", status: "modified", staged });
    expect(changesLine([], false)).toBe("Nothing changed since the last commit.");
    expect(changesLine([change(true), change(false)], false)).toBe("2 files changed, 1 staged");
    expect(changesLine([change(false)], true)).toBe("1+ file changed");
  });

  it("says how a branch stands to what it follows", () => {
    const branch = (over: Partial<GitBranch>): GitBranch => ({
      name: "feat/x",
      head: "abc",
      subject: "Work",
      author: "Forge",
      at_ms: 0,
      ...over,
    });
    expect(trackLine(branch({}))).toBe("Not pushed");
    expect(trackLine(branch({ upstream: "origin/feat/x", ahead: 0, behind: 0 }))).toBe("Same as origin/feat/x");
    expect(trackLine(branch({ upstream: "origin/feat/x", ahead: 2, behind: 1 }))).toBe(
      "2 ahead, 1 behind of origin/feat/x",
    );
    expect(trackLine(branch({ upstream: "origin/feat/x" }))).toBe("origin/feat/x is gone");
    expect(trackLine(branch({ remote: true }))).toBe("");
  });

  it("says how long ago", () => {
    const now = 1_700_000_000_000;
    expect(ago(now - 20_000, now)).toBe("just now");
    expect(ago(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(ago(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(ago(now - 2 * 86_400_000, now)).toBe("2 d ago");
  });
});
