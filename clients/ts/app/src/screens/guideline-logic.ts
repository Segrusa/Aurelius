// What the guideline screens compute, without a screen: counters and limits, the groups of the Links
// dialog, the largest block, the texts, and markdown that cannot load anything. Port of
// clients/desktop/src/screens/guidelines/logic.rs.

import type {
  EffectiveGuidelines,
  EmployeeSummary,
  GuidelineAffected,
  GuidelineProposalView,
  GuidelineTarget,
  GuidelineTooLarge,
  GuidelineVersionView,
  GuidelineView,
  ProjectSummary,
  RunGuideline,
} from "@aurelius/protocol";

export const TEXT_LIMIT = 8000;
export const RUN_LIMIT = 16000;
export const TITLE_LIMIT = 120;
const WARN_PERCENT = 80;
export const SIDE_MIN_W = 1100;

export const NOT_LINKED = "— not linked";
export const ABOUT = "Rules every linked run gets, on top of its instructions.";
export const EMPTY_ABOUT =
  "Guidelines are working rules (how to write Rust, how to use worktrees) that you link to employees or projects. They come on top of an employee's instructions in every run.";
export const EMPTY_PROPOSE = "Employees can also propose one; the bell tells you.";
export const BANNER =
  "Aurelius added a starter set with the team's current rules. Check them and their links; they apply from the next run.";
export const GROUP_NOTE =
  "Selecting a group links its current members one by one. New reports are not linked automatically.";
export const READ_ONLY = "Only a board member changes guidelines. Sign in as one to edit.";

export const paneHeight = (viewport: number) => Math.max(viewport - 430, 320);

/** "6 120", "16 000", "940". */
export function count(n: number): string {
  return String(n).replace(/\B(?=(\d{3})+$)/g, " ");
}

export type Level = "calm" | "close" | "over";

export function level(chars: number, limit: number): Level {
  if (chars > limit) return "over";
  return chars * 100 >= limit * WARN_PERCENT ? "close" : "calm";
}

/** The text as the server stores it: blank lines at the start and whitespace at the end go. */
export const stored = (text: string) => text.replace(/^[\n\r]+/, "").trimEnd();

/** Characters as the limit counts them: code points, as SQLite's `length()`. */
export const chars = (text: string) => [...stored(text)].length;

export const counterLine = (n: number) => `${count(n)} / ${count(TEXT_LIMIT)} characters`;

export function counterNote(n: number): string | null {
  switch (level(n, TEXT_LIMIT)) {
    case "calm":
      return null;
    case "close":
      return "Close to the limit";
    case "over":
      return `${count(n - TEXT_LIMIT)} characters over the limit of ${count(TEXT_LIMIT)}`;
  }
}

/** Only an estimate: 4 characters each. */
export function tokens(n: number): string {
  const t = n / 4;
  return t >= 1000 ? `≈ ${(t / 1000).toFixed(1)}k tokens` : `≈ ${Math.round(t)} tokens`;
}

export interface Names {
  employees: EmployeeSummary[];
  projects: ProjectSummary[];
}

export const employeeName = (names: Names, id: string) =>
  names.employees.find((e) => e.id === id)?.name ?? "An employee";
export const projectName = (names: Names, id: string) => names.projects.find((p) => p.id === id)?.name ?? "a project";

export function targetName(names: Names, target: GuidelineTarget): string {
  if (target.kind === "employee") {
    const e = names.employees.find((x) => x.id === target.id);
    return `${employeeName(names, target.id)}${!e || e.status === "archived" ? " (archived)" : ""}`;
  }
  const p = names.projects.find((x) => x.id === target.id);
  return `▸ ${projectName(names, target.id)}${!p || p.status === "archived" ? " (archived)" : ""}`;
}

const byLower = (a: string, b: string) => {
  const [x, y] = [a.toLowerCase(), b.toLowerCase()];
  return x < y ? -1 : x > y ? 1 : 0;
};

/** Employees first, then projects, each by name. */
export function sortedTargets(links: GuidelineTarget[], names: Names): GuidelineTarget[] {
  return [...links].sort(
    (a, b) =>
      Number(a.kind === "project") - Number(b.kind === "project") ||
      byLower(targetName(names, a), targetName(names, b)),
  );
}

/** At most three, then "+n"; and the full list for the tooltip. */
export function appliesTo(links: GuidelineTarget[], names: Names): { shown: string; all: string } {
  if (links.length === 0) return { shown: NOT_LINKED, all: "Not linked: no run gets it." };
  const all = sortedTargets(links, names).map((t) => targetName(names, t));
  const more = all.length > 3 ? ` +${all.length - 3}` : "";
  return { shown: `${all.slice(0, 3).join(", ")}${more}`, all: all.join(", ") };
}

export type Sort = "title" | "size" | "updated";

/** `@Forge` finds what is linked to Forge, `▸aurelius` or `>aurelius` a project; else the titles. */
export function shown(list: GuidelineView[], query: string, sort: Sort, names: Names): GuidelineView[] {
  const q = query.trim().toLowerCase();
  const out = list.filter((g) => {
    if (q.startsWith("@")) {
      const who = q.slice(1).trim();
      return g.links.some((t) => t.kind === "employee" && employeeName(names, t.id).toLowerCase().includes(who));
    }
    if (q.startsWith("▸") || q.startsWith(">")) {
      const where = q.slice(1).trim();
      return g.links.some((t) => t.kind === "project" && projectName(names, t.id).toLowerCase().includes(where));
    }
    return g.title.toLowerCase().includes(q);
  });
  if (sort === "title") out.sort((a, b) => byLower(a.title, b.title));
  if (sort === "size") out.sort((a, b) => b.chars - a.chars || byLower(a.title, b.title));
  if (sort === "updated") out.sort((a, b) => b.updated_at_ms - a.updated_at_ms);
  return out;
}

function fenceMarker(body: string): { ch: string; n: number; rest: string } | null {
  const ch = body[0];
  if (ch !== "`" && ch !== "~") return null;
  let n = 0;
  while (body[n] === ch) n++;
  const rest = body.slice(n);
  return n >= 3 && !(ch === "`" && rest.includes("`")) ? { ch, n, rest } : null;
}

function escapeInline(line: string): string {
  const c = [...line];
  const run = (from: number) => {
    let n = 0;
    while (c[from + n] === "`") n++;
    return n;
  };
  let out = "";
  let i = 0;
  while (i < c.length) {
    const ch = c[i] as string;
    if (ch === "`") {
      const n = run(i);
      let close = -1;
      for (let j = i + n; j < c.length; j++) {
        if (c[j] === "`" && c[j - 1] !== "`" && run(j) === n) {
          close = j;
          break;
        }
      }
      if (close >= 0) {
        for (const x of c.slice(i, close + n)) out += x === "|" ? "\\|" : x;
        i = close + n;
      } else {
        out += c.slice(i, i + n).join("");
        i += n;
      }
      continue;
    }
    if (ch === "\\" && i + 1 < c.length) {
      out += ch + c[i + 1];
      i += 2;
      continue;
    }
    if (ch === "<" || ch === "[") out += "\\";
    out += ch;
    i++;
  }
  return out;
}

/**
 * Markdown for a preview of a guideline or a proposal: no HTML, nothing that loads by itself, no link
 * that hides its address. Outside code `<` and `[` are escaped, so they show as the text they are. In a
 * code span `|` becomes `\|`, because a table splits its cells before it sees code spans.
 */
export function safeMarkdown(text: string): string {
  let out = "";
  let fence: { ch: string; n: number } | null = null;
  for (const line of text.match(/[^\n]*\n|[^\n]+$/g) ?? []) {
    const body = line.replace(/^ +/, "");
    const marker = line.length - body.length < 4 ? fenceMarker(body) : null;
    if (!fence && marker) {
      fence = { ch: marker.ch, n: marker.n };
      out += line;
    } else if (fence && marker && marker.ch === fence.ch && marker.n >= fence.n && marker.rest.trim() === "") {
      fence = null;
      out += line;
    } else {
      out += fence ? line : escapeInline(line);
    }
  }
  return out;
}

export interface Group {
  boss: string | null;
  label: string;
  members: string[];
}

/** Who can be linked, by whom they report to. Archived employees only while `keep` holds them. */
export function groups(employees: EmployeeSummary[], keep: string[]): Group[] {
  const members = employees
    .filter((e) => e.status !== "archived" || keep.includes(e.id))
    .sort((a, b) => byLower(a.name, b.name));
  const out: Group[] = [];
  for (const e of members) {
    const known = e.reports_to && employees.some((x) => x.id === e.reports_to) ? e.reports_to : null;
    const group = out.find((g) => g.boss === known);
    if (group) group.members.push(e.id);
    else
      out.push({
        boss: known,
        label: known ? `Reports to ${employees.find((x) => x.id === known)?.name ?? "someone"}` : "Reports to no one",
        members: [e.id],
      });
  }
  return out.sort((a, b) => Number(a.boss !== null) - Number(b.boss !== null) || byLower(a.label, b.label));
}

export const targetKey = (t: GuidelineTarget) => `${t.kind}:${t.id}`;

export type Tri = "none" | "some" | "all";

export function tri(members: string[], ticked: Set<string>): { ticked: number; of: number; state: Tri } {
  const n = members.filter((m) => ticked.has(`employee:${m}`)).length;
  return { ticked: n, of: members.length, state: n === 0 ? "none" : n === members.length ? "all" : "some" };
}

export interface LinkChange {
  target: GuidelineTarget;
  link: boolean;
}

const asTarget = (key: string): GuidelineTarget => {
  const at = key.indexOf(":");
  return { kind: key.slice(0, at) as GuidelineTarget["kind"], id: key.slice(at + 1) };
};

/** The links first, then the unlinks, each in a stable order. */
export function linkChanges(before: Set<string>, after: Set<string>): LinkChange[] {
  const add = [...after].filter((k) => !before.has(k)).sort();
  const remove = [...before].filter((k) => !after.has(k)).sort();
  return [
    ...add.map((k) => ({ target: asTarget(k), link: true })),
    ...remove.map((k) => ({ target: asTarget(k), link: false })),
  ];
}

/** Without a project, and each active project that has a guideline linked to it. */
export function contexts(list: GuidelineView[], projects: ProjectSummary[]): (string | null)[] {
  const linked = projects
    .filter(
      (p) =>
        p.status !== "archived" &&
        list.some((g) => !g.archived && g.links.some((t) => t.kind === "project" && t.id === p.id)),
    )
    .sort((a, b) => byLower(a.name, b.name));
  return [null, ...linked.map((p) => p.id)];
}

export const setKey = (employee: string | null, project: string | null) => `${employee ?? ""}|${project ?? ""}`;

export type Sets = Map<string, EffectiveGuidelines>;

export function wanted(employees: EmployeeSummary[], where: (string | null)[]): [string, string | null][] {
  return employees
    .filter((e) => e.status !== "archived")
    .flatMap((e) => where.map((p): [string, string | null] => [e.id, p]));
}

/** The block of a set once `g` has `n` characters. A guideline counts once. */
export function blockWith(set: EffectiveGuidelines, g: string | null, n: number, joins: boolean): number {
  const had = g === null ? undefined : set.guidelines.find((x) => x.id === g);
  if (had) return Math.max(0, set.total_chars - had.chars) + n;
  return joins ? set.total_chars + n : set.total_chars;
}

export interface Block {
  employee: string | null;
  project: string | null;
  chars: number;
}

const keyParts = (key: string): [string | null, string | null] => {
  const [e, p] = key.split("|");
  return [e || null, p || null];
};

/** The largest block a run would get with `g` (null: a new one) at `n` characters linked to `links`. */
export function largestWith(sets: Sets, g: string | null, n: number, links: GuidelineTarget[]): Block | null {
  let best: Block | null = null;
  for (const [key, set] of sets) {
    const [employee, project] = keyParts(key);
    const concerned =
      (g !== null && set.guidelines.some((x) => x.id === g)) ||
      (employee !== null && links.some((t) => t.kind === "employee" && t.id === employee)) ||
      (project !== null && links.some((t) => t.kind === "project" && t.id === project));
    if (!concerned) continue;
    const block = { employee, project, chars: blockWith(set, g, n, true) };
    const rank = (b: Block): [number, number, number] => [
      b.chars,
      Number(b.employee !== null),
      Number(b.project === null),
    ];
    if (!best) best = block;
    else {
      const [x, y] = [rank(block), rank(best)];
      if (x[0] > y[0] || (x[0] === y[0] && (x[1] > y[1] || (x[1] === y[1] && x[2] >= y[2])))) best = block;
    }
  }
  return best;
}

function maxBlock(blocks: Block[]): Block | null {
  return blocks.reduce<Block | null>((best, b) => (!best || b.chars >= best.chars ? b : best), null);
}

/** The largest block of an employee over its contexts, now or with `g` linked to it. */
export function employeeBlock(
  sets: Sets,
  employee: string,
  where: (string | null)[],
  g?: { id: string | null; chars: number },
): Block | null {
  return maxBlock(
    where.flatMap((project) => {
      const set = sets.get(setKey(employee, project));
      if (!set) return [];
      return [{ employee, project, chars: g ? blockWith(set, g.id, g.chars, true) : set.total_chars }];
    }),
  );
}

/** The largest block in a project with `g` linked to it. */
export function projectBlock(
  sets: Sets,
  project: string,
  where: (string | null)[],
  employees: EmployeeSummary[],
  g: string | null,
  n: number,
): Block | null {
  const at = where.includes(project) ? project : null;
  return maxBlock(
    employees
      .filter((e) => e.status !== "archived")
      .flatMap((e) => {
        const set = sets.get(setKey(e.id, at));
        return set ? [{ employee: e.id, project, chars: blockWith(set, g, n, true) }] : [];
      }),
  );
}

function sentenceWithMax(block: Block, limit: number, names: Names): string {
  const n = count(block.chars);
  const max = count(limit);
  if (block.employee && block.project)
    return `${employeeName(names, block.employee)} would get ${n} characters in ${projectName(names, block.project)} (max ${max}).`;
  if (block.employee)
    return `${employeeName(names, block.employee)} would get ${n} characters without a project (max ${max}).`;
  if (block.project) return `A run in ${projectName(names, block.project)} would get ${n} characters (max ${max}).`;
  return `A run would get ${n} characters (max ${max}).`;
}

export const blockSentence = (block: Block, names: Names) => sentenceWithMax(block, RUN_LIMIT, names);

export function blockWhere(block: Block, names: Names): string {
  if (block.employee && block.project)
    return `${employeeName(names, block.employee)} in ${projectName(names, block.project)}`;
  if (block.employee) return `${employeeName(names, block.employee)} without a project`;
  return block.project ? `a run in ${projectName(names, block.project)}` : "a run";
}

export const tooLargeText = (t: GuidelineTooLarge, names: Names) =>
  `${sentenceWithMax({ employee: t.employee_id ?? null, project: t.project_id ?? null, chars: t.chars }, t.limit, names)} Shorten this text or unlink something.`;

export interface Failure {
  code?: string | undefined;
  message: string;
  tooLarge?: GuidelineTooLarge | null | undefined;
  matchedItems?: string[] | undefined;
}

/** A refused write in the user's words. A guideline goes into every run, so a secret is never confirmed. */
export function failureText(f: Failure, names: Names): string {
  switch (f.code) {
    case "guideline_too_large":
      return f.tooLarge ? tooLargeText(f.tooLarge, names) : `${f.message} Shorten this text or unlink something.`;
    case "guideline_title_taken":
      return "Another guideline already has this title. Choose another title.";
    case "version_conflict":
      return "Someone saved a newer version while you were editing. Compare, or discard yours.";
    case "looks_like_secret":
      return f.matchedItems?.length
        ? `This looks like a vault secret: ${f.matchedItems.join(", ")}. Take the value out: guidelines go into every run.`
        : "This looks like a vault secret. Take the value out: guidelines go into every run.";
    default:
      return f.message;
  }
}

export function sourceLabel(v: GuidelineVersionView, proposals: GuidelineProposalView[], names: Names): string {
  switch (v.source) {
    case "seed":
      return "starter set";
    case "created":
      return "created";
    case "edited":
      return "edited";
    case "proposal": {
      const p = proposals.find((x) => x.id === v.proposal_id);
      return p ? `approved proposal from ${employeeName(names, p.proposed_by)}` : "approved proposal";
    }
    case "restored":
      return v.restored_from ? `restored from version ${v.restored_from}` : "restored";
  }
}

export const runLine = (list: RunGuideline[]) =>
  list.length > 0 ? `Guidelines: ${list.map((g) => `${g.title} v${g.version}`).join(", ")}` : null;

export function affectedLine(a: GuidelineAffected): string {
  const who =
    a.employees === 0
      ? "Applies from the next run; no employee gets it now."
      : a.employees === 1
        ? "Applies from the next run of 1 employee."
        : `Applies from the next run of ${a.employees} employees.`;
  if (a.conversations === 0) return who;
  return a.conversations === 1
    ? `${who} 1 conversation starts a new session.`
    : `${who} ${a.conversations} conversations start a new session.`;
}

export function proposalHeadline(p: GuidelineProposalView, current: GuidelineView | null, names: Names): string {
  const who = employeeName(names, p.proposed_by);
  if (!p.guideline_id) return `${who} proposes a new guideline “${p.title}”`;
  return `${who} proposes a change to “${current?.title ?? p.title}”`;
}

export function stale(p: GuidelineProposalView, current: GuidelineView | null): { base: number; now: number } | null {
  if (p.guideline_id && p.base_version != null && current && current.version !== p.base_version) {
    return { base: p.base_version, now: current.version };
  }
  return null;
}

export function approveProblem(
  p: GuidelineProposalView,
  current: GuidelineView | null,
  links: GuidelineTarget[],
  sets: Sets,
  names: Names,
): string | null {
  if (stale(p, current)) return "It changed since this was proposed: reject it.";
  const n = chars(p.text);
  if (n > TEXT_LIMIT) return counterNote(n);
  const largest = largestWith(sets, p.guideline_id ?? null, n, links);
  return largest && largest.chars > RUN_LIMIT ? blockSentence(largest, names) : null;
}

/** While a seeded guideline is as the seed made it and this board member has not closed it. */
export function bannerDue(list: GuidelineView[], me: string | null, done: string[]): boolean {
  return me !== null && !done.includes(me) && list.some((g) => g.version === 1 && !g.updated_by);
}

/** Why Save is off, once there is something to say, and not what the counter already says. */
export function editorMessage(failure: string | null, problem: string | null, title: string, n: number): string | null {
  if (failure) return failure;
  return problem && n > 0 && title.trim() !== "" && counterNote(n) !== problem ? problem : null;
}

export function createdLine(title: string, links: GuidelineTarget[], names: Names): string {
  return links.length === 0
    ? `Saved “${title}” as version 1. Link it to use it.`
    : `Saved “${title}” as version 1. It applies to ${appliesTo(links, names).shown} from the next run.`;
}

export function saveProblem(title: string, text: string, largest: Block | null, names: Names): string | null {
  const n = chars(text);
  if (title.trim() === "") return "Give it a title.";
  if ([...title.trim()].length > TITLE_LIMIT) return `A title has at most ${TITLE_LIMIT} characters.`;
  if (n === 0) return "A guideline needs a text. To stop using one, archive it.";
  if (n > TEXT_LIMIT) return counterNote(n);
  return largest && largest.chars > RUN_LIMIT
    ? `${blockSentence(largest, names)} Shorten this text or unlink something.`
    : null;
}

/** Only what changed; null when nothing did. */
export function changes(
  original: { title: string; text: string },
  now: { title: string; text: string },
): { title?: string; text?: string } | null {
  const out: { title?: string; text?: string } = {};
  if (original.title.trim() !== now.title.trim()) out.title = now.title.trim();
  if (stored(original.text) !== stored(now.text)) out.text = stored(now.text);
  return out.title !== undefined || out.text !== undefined ? out : null;
}

/** A line-by-line comparison (longest common subsequence). Very long texts: all removed, then all added. */
export function lineDiff(a: string, b: string): { mark: " " | "-" | "+"; line: string }[] {
  const lines = (t: string) => {
    const out = t.split(/\r?\n/);
    if (out.at(-1) === "") out.pop();
    return out;
  };
  const x = lines(a);
  const y = lines(b);
  if (x.length * y.length > 4_000_000) {
    return [...x.map((line) => ({ mark: "-" as const, line })), ...y.map((line) => ({ mark: "+" as const, line }))];
  }
  const width = y.length + 1;
  const lcs = new Uint32Array((x.length + 1) * width);
  for (let i = x.length - 1; i >= 0; i--) {
    for (let j = y.length - 1; j >= 0; j--) {
      lcs[i * width + j] =
        x[i] === y[j]
          ? (lcs[(i + 1) * width + j + 1] as number) + 1
          : Math.max(lcs[(i + 1) * width + j] as number, lcs[i * width + j + 1] as number);
    }
  }
  const out: { mark: " " | "-" | "+"; line: string }[] = [];
  let [i, j] = [0, 0];
  while (i < x.length || j < y.length) {
    if (i < x.length && j < y.length && x[i] === y[j]) {
      out.push({ mark: " ", line: x[i] as string });
      i++;
      j++;
    } else if (
      i < x.length &&
      (j >= y.length || (lcs[(i + 1) * width + j] as number) >= (lcs[i * width + j + 1] as number))
    ) {
      out.push({ mark: "-", line: x[i] as string });
      i++;
    } else {
      out.push({ mark: "+", line: y[j] as string });
      j++;
    }
  }
  return out;
}
