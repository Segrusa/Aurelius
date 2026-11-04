import type {
  EffectiveGuidelines,
  EmployeeSummary,
  GuidelineProposalView,
  GuidelineView,
  ProjectSummary,
} from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  affectedLine,
  appliesTo,
  approveProblem,
  bannerDue,
  blockSentence,
  blockWhere,
  changes,
  chars,
  contexts,
  count,
  counterLine,
  counterNote,
  createdLine,
  editorMessage,
  employeeBlock,
  failureText,
  groups,
  largestWith,
  level,
  lineDiff,
  linkChanges,
  paneHeight,
  projectBlock,
  proposalHeadline,
  runLine,
  type Sets,
  safeMarkdown,
  saveProblem,
  setKey,
  shown,
  stale,
  tokens,
  tri,
  wanted,
} from "./guideline-logic";

const emp = (id: string, name: string, over: Partial<EmployeeSummary> = {}) =>
  ({ id, name, status: "active", ...over }) as EmployeeSummary;
const proj = (id: string, name: string, over: Partial<ProjectSummary> = {}) =>
  ({ id, name, status: "active", ...over }) as ProjectSummary;
const guide = (id: string, title: string, over: Partial<GuidelineView> = {}) =>
  ({ id, title, chars: 100, links: [], version: 1, archived: false, updated_at_ms: 0, ...over }) as GuidelineView;
const set = (list: [string, number][]): EffectiveGuidelines =>
  ({
    guidelines: list.map(([id, n]) => ({ id, chars: n })),
    total_chars: list.reduce((a, [, n]) => a + n, 0),
  }) as EffectiveGuidelines;
const names = {
  employees: [emp("forge", "Forge"), emp("flux", "Flux")],
  projects: [proj("au", "aurelius"), proj("web", "web")],
};

describe("counters", () => {
  it("writes thousands apart", () => {
    expect([count(940), count(6120), count(1234567)]).toEqual(["940", "6 120", "1 234 567"]);
  });
  it("levels", () => {
    expect([level(6399, 8000), level(6400, 8000), level(8000, 8000), level(8001, 8000)]).toEqual([
      "calm",
      "close",
      "close",
      "over",
    ]);
    expect([level(12799, 16000), level(12800, 16000)]).toEqual(["calm", "close"]);
  });
  it("says it", () => {
    expect(counterLine(6120)).toBe("6 120 / 8 000 characters");
    expect(counterNote(6000)).toBeNull();
    expect(counterNote(7000)).toBe("Close to the limit");
    expect(counterNote(9240)).toBe("1 240 characters over the limit of 8 000");
    expect(tokens(18940)).toBe("≈ 4.7k tokens");
    expect(chars("\n\n  code\n\n")).toBe(6);
    expect([paneHeight(900), paneHeight(1400), paneHeight(600)]).toEqual([470, 970, 320]);
  });
});

describe("saving", () => {
  it("names the problem", () => {
    expect(saveProblem("", "x", null, names)).toBe("Give it a title.");
    expect(saveProblem("Rust", "  \n ", null, names)).toContain("archive");
    expect(saveProblem("Rust", "a".repeat(8001), null, names)).toBe("1 characters over the limit of 8 000");
    expect(saveProblem("Rust", "a".repeat(8000), null, names)).toBeNull();
    expect(saveProblem("x".repeat(121), "a", null, names)).toBe("A title has at most 120 characters.");
    expect(saveProblem("Rust", "a", { employee: null, project: null, chars: 16000 }, names)).toBeNull();
    expect(saveProblem("Rust", "a", { employee: null, project: null, chars: 16001 }, names)).toBe(
      "A run would get 16 001 characters (max 16 000). Shorten this text or unlink something.",
    );
  });
  it("sends only what changed", () => {
    expect(changes({ title: "Rust", text: "a\n" }, { title: "Rust ", text: "a" })).toBeNull();
    expect(changes({ title: "Rust", text: "a" }, { title: "Rust 2", text: "a" })).toEqual({ title: "Rust 2" });
    expect(changes({ title: "Rust", text: "a" }, { title: "Rust", text: "\nb\n" })).toEqual({ text: "b" });
  });
  it("says one thing", () => {
    expect(editorMessage("failed", "problem", "T", 5)).toBe("failed");
    expect(editorMessage(null, "1 characters over the limit of 8 000", "T", 8001)).toBeNull();
    expect(editorMessage(null, "problem", "", 5)).toBeNull();
    expect(editorMessage(null, "problem", "T", 0)).toBeNull();
    expect(editorMessage(null, "problem", "T", 5)).toBe("problem");
  });
  it("words a new one", () => {
    expect(createdLine("Commit messages", [], names)).toBe("Saved “Commit messages” as version 1. Link it to use it.");
    expect(
      createdLine(
        "Commit messages",
        [
          { kind: "employee", id: "flux" },
          { kind: "project", id: "x" },
        ],
        {
          employees: names.employees,
          projects: [proj("x", "Demo")],
        },
      ),
    ).toBe("Saved “Commit messages” as version 1. It applies to Flux, ▸ Demo from the next run.");
  });
  it("words a refusal", () => {
    const n = { employees: [emp("b", "Bridge")], projects: [proj("au", "aurelius")] };
    expect(
      failureText(
        {
          code: "guideline_too_large",
          message: "",
          tooLarge: { employee_id: "b", project_id: "au", chars: 25300, limit: 16000 },
        },
        n,
      ),
    ).toBe("Bridge would get 25 300 characters in aurelius (max 16 000). Shorten this text or unlink something.");
    expect(
      failureText(
        { code: "guideline_too_large", message: "", tooLarge: { project_id: "au", chars: 17000, limit: 16000 } },
        n,
      ),
    ).toContain("A run in aurelius would get 17 000");
    expect(failureText({ code: "guideline_title_taken", message: "" }, n)).toBe(
      "Another guideline already has this title. Choose another title.",
    );
    expect(failureText({ code: "version_conflict", message: "" }, n)).toBe(
      "Someone saved a newer version while you were editing. Compare, or discard yours.",
    );
    expect(failureText({ code: "looks_like_secret", message: "", matchedItems: ["GitHub token"] }, n)).toBe(
      "This looks like a vault secret: GitHub token. Take the value out: guidelines go into every run.",
    );
  });
});

describe("safe markdown", () => {
  it.each([
    ["Use `Vec<T>` and `[a]`.\n", "Use `Vec<T>` and `[a]`.\n"],
    ["``a ` <b>``\n", "``a ` <b>``\n"],
    ["```html\n<img src=x>\n[a](b)\n```\n<b>\n", "```html\n<img src=x>\n[a](b)\n```\n\\<b>\n"],
    ["~~~\n<x>\n~~~~\n<y>", "~~~\n<x>\n~~~~\n\\<y>"],
    ["a ` <b> \\[c]", "a ` \\<b> \\[c]"],
    ["    ```\n<b>", "    ```\n\\<b>"],
    ["## Errors\n- **No** `unwrap()`\n", "## Errors\n- **No** `unwrap()`\n"],
    ["`a|b` c|d\n", "`a\\|b` c|d\n"],
    ["![i](https://e.invalid/p)", "!\\[i](https://e.invalid/p)"],
  ])("%s", (input, output) => {
    expect(safeMarkdown(input)).toBe(output);
  });
});

describe("library", () => {
  const list = [
    guide("r", "Rust", {
      chars: 900,
      updated_at_ms: 2,
      links: [
        { kind: "employee", id: "forge" },
        { kind: "project", id: "au" },
      ],
    }),
    guide("g", "git & worktrees", { chars: 500, updated_at_ms: 1, links: [{ kind: "project", id: "au" }] }),
    guide("w", "Writing docs", { chars: 100, updated_at_ms: 3 }),
  ];
  const titles = (q: string, s: "title" | "size" | "updated") => shown(list, q, s, names).map((g) => g.title);
  it("orders and finds", () => {
    expect(titles("", "title")).toEqual(["git & worktrees", "Rust", "Writing docs"]);
    expect(titles("", "size")).toEqual(["Rust", "git & worktrees", "Writing docs"]);
    expect(titles("", "updated")).toEqual(["Writing docs", "Rust", "git & worktrees"]);
    expect(titles("RUST", "title")).toEqual(["Rust"]);
    expect(titles("@forge", "title")).toEqual(["Rust"]);
    expect(titles("▸aurel", "title")).toEqual(["git & worktrees", "Rust"]);
    expect(titles(">aurel", "title")).toEqual(["git & worktrees", "Rust"]);
  });
  it("names who gets it", () => {
    const n = {
      employees: ["Bridge", "Forge", "Nexus", "Schema"].map((x) => emp(x, x)),
      projects: [proj("au", "aurelius")],
    };
    const links = [
      ...n.employees.map((e) => ({ kind: "employee" as const, id: e.id })),
      { kind: "project" as const, id: "au" },
    ];
    expect(appliesTo(links, n)).toEqual({
      shown: "Bridge, Forge, Schema +2".replace("Schema", "Nexus"),
      all: "Bridge, Forge, Nexus, Schema, ▸ aurelius",
    });
    expect(appliesTo([], n).shown).toBe("— not linked");
    expect(appliesTo([{ kind: "employee", id: "gone" }], n).shown).toBe("An employee (archived)");
  });
  it("banner", () => {
    const seeded = guide("a", "A", { version: 1 });
    const edited = guide("b", "B", {
      version: 2,
      updated_by: { id: "x", username: "x", created_at_ms: 0 },
    } as Partial<GuidelineView>);
    expect(bannerDue([seeded, edited], "u1", [])).toBe(true);
    expect(bannerDue([seeded], "u1", ["u1"])).toBe(false);
    expect(bannerDue([seeded], "u2", ["u1"])).toBe(true);
    expect(bannerDue([edited], "u1", [])).toBe(false);
    expect(bannerDue([], null, [])).toBe(false);
  });
});

describe("links", () => {
  it("groups by whom they report to", () => {
    const list = [
      emp("t", "Titan"),
      emp("f", "Forge", { reports_to: "t" }),
      emp("o", "Orphan", { reports_to: "gone" }),
      emp("p", "Paused", { status: "paused" }),
      emp("a", "Gone", { status: "archived" }),
    ];
    expect(groups(list, []).map((g) => g.label)).toEqual(["Reports to no one", "Reports to Titan"]);
    expect(groups(list, [])[0]?.members).toEqual(["o", "p", "t"]);
    expect(groups(list, ["a"])[0]?.members).toEqual(["a", "o", "p", "t"]);
  });
  it("counts a group", () => {
    const key = (...ids: string[]) => new Set(ids.map((i) => `employee:${i}`));
    expect(tri(["a", "b", "c"], key()).state).toBe("none");
    expect(tri(["a", "b", "c"], key("a"))).toEqual({ ticked: 1, of: 3, state: "some" });
    expect(tri(["a", "b", "c"], key("a", "b", "c")).state).toBe("all");
  });
  it("adds first, then removes", () => {
    const changed = linkChanges(new Set(["employee:a", "project:x"]), new Set(["employee:a", "employee:b"]));
    expect(changed).toEqual([
      { target: { kind: "employee", id: "b" }, link: true },
      { target: { kind: "project", id: "x" }, link: false },
    ]);
    expect(linkChanges(new Set(["employee:a"]), new Set(["employee:a"]))).toEqual([]);
  });
});

describe("the largest block", () => {
  const rust = guide("rust", "Rust", {
    chars: 6000,
    links: [
      { kind: "employee", id: "forge" },
      { kind: "project", id: "au" },
    ],
  });
  const git = guide("git", "Git", { chars: 2000, links: [{ kind: "employee", id: "forge" }] });
  const staff = [emp("forge", "Forge"), emp("flux", "Flux"), emp("old", "Old", { status: "archived" })];
  const places = [proj("au", "aurelius"), proj("web", "web"), proj("arch", "Arch", { status: "archived" })];
  const n = { employees: staff, projects: places };
  const sets: Sets = new Map([
    [
      setKey("forge", null),
      set([
        ["rust", 6000],
        ["git", 2000],
      ]),
    ],
    [
      setKey("forge", "au"),
      set([
        ["rust", 6000],
        ["git", 2000],
      ]),
    ],
    [setKey("flux", null), set([])],
    [setKey("flux", "au"), set([["rust", 6000]])],
  ]);
  it("knows where to look", () => {
    expect(contexts([rust, git], places)).toEqual([null, "au"]);
    expect(wanted(staff, [null, "au"])).toHaveLength(4);
  });
  it("finds the largest with a change", () => {
    expect(largestWith(sets, "rust", 7900, [])?.chars).toBe(9900);
    expect(largestWith(sets, null, 8000, [{ kind: "employee", id: "flux" }])).toEqual({
      employee: "flux",
      project: "au",
      chars: 14000,
    });
  });
  it("per employee and per project", () => {
    expect(employeeBlock(sets, "forge", [null, "au"], { id: null, chars: 8000 })?.chars).toBe(16000);
    expect(employeeBlock(sets, "forge", [null, "au"])?.chars).toBe(8000);
    expect(projectBlock(sets, "web", [null, "au"], staff, null, 9000)).toEqual({
      employee: "forge",
      project: "web",
      chars: 17000,
    });
  });
  it("words it", () => {
    expect(blockSentence({ employee: "forge", project: "web", chars: 17000 }, n)).toBe(
      "Forge would get 17 000 characters in web (max 16 000).",
    );
    expect(blockWhere({ employee: "forge", project: null, chars: 1 }, n)).toBe("Forge without a project");
  });
});

describe("proposals and lines", () => {
  const n = { employees: [emp("f", "Forge")], projects: [] };
  const prop = (over: Partial<GuidelineProposalView>) =>
    ({
      id: "p",
      proposed_by: "f",
      title: "Async Rust",
      text: "x",
      suggested_links: [],
      ...over,
    }) as GuidelineProposalView;
  const rust = guide("r", "Rust", { version: 3 });
  it("heads and goes stale", () => {
    expect(proposalHeadline(prop({ guideline_id: "r" }), rust, n)).toBe("Forge proposes a change to “Rust”");
    expect(proposalHeadline(prop({}), null, n)).toBe("Forge proposes a new guideline “Async Rust”");
    expect(stale(prop({ guideline_id: "r", base_version: 3 }), rust)).toBeNull();
    expect(stale(prop({ guideline_id: "r", base_version: 3 }), guide("r", "Rust", { version: 4 }))).toEqual({
      base: 3,
      now: 4,
    });
  });
  it("may not be approved over a limit", () => {
    const sets: Sets = new Map([
      [
        setKey("f", null),
        set([
          ["r", 6000],
          ["o", 9000],
        ]),
      ],
    ]);
    const change = (len: number) => prop({ guideline_id: "r", base_version: 3, text: "a".repeat(len) });
    expect(approveProblem(change(7000), rust, [], sets, n)).toBeNull();
    expect(approveProblem(change(7001), rust, [], sets, n)).toBe(
      "Forge would get 16 001 characters without a project (max 16 000).",
    );
    expect(approveProblem(change(8001), rust, [], sets, n)).toBe("1 characters over the limit of 8 000");
    expect(approveProblem(prop({ guideline_id: "r", base_version: 2 }), rust, [], sets, n)).toContain("changed");
  });
  it("words who is affected", () => {
    expect(affectedLine({ employees: 3, conversations: 0 } as never)).toBe("Applies from the next run of 3 employees.");
    expect(affectedLine({ employees: 1, conversations: 2 } as never)).toBe(
      "Applies from the next run of 1 employee. 2 conversations start a new session.",
    );
    expect(affectedLine({ employees: 0, conversations: 0 } as never)).toBe(
      "Applies from the next run; no employee gets it now.",
    );
    expect(
      runLine([
        { title: "Rust", version: 3 },
        { title: "Git & worktrees", version: 4 },
      ] as never),
    ).toBe("Guidelines: Rust v3, Git & worktrees v4");
    expect(runLine([])).toBeNull();
  });
  it("compares lines", () => {
    expect(
      lineDiff("Be brief.\nUse Dutch.\nTest first.", "Be brief.\nUse English.\nTest first.\nAsk before pushing."),
    ).toEqual([
      { mark: " ", line: "Be brief." },
      { mark: "-", line: "Use Dutch." },
      { mark: "+", line: "Use English." },
      { mark: " ", line: "Test first." },
      { mark: "+", line: "Ask before pushing." },
    ]);
    expect(lineDiff("", "New.")).toEqual([{ mark: "+", line: "New." }]);
    expect(lineDiff("Old.", "")).toEqual([{ mark: "-", line: "Old." }]);
  });
});
