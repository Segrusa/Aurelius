// The Team chat without a window: who groups with whom, where the separators go, what `@` means, the
// channel list's order, and how a thread and a reaction read. Port of team_feed.rs, discussion.rs,
// threads.rs, progress.rs and the pure parts of chat.rs.

import type { ActivityView, ChannelView, MessageView, ReactionView, SystemLine } from "@aurelius/protocol";

export const GROUP_WINDOW_MS = 5 * 60 * 1000;
const DAY_MS = 86_400_000;
export const SHOW_MORE_AFTER = 15;
export const REACTIONS = ["✓", "👀", "👍", "🎉", "❓", "❗", "🙏", "🚀"];
/** Those the bar above a message offers at one click. */
export const QUICK_REACTIONS = ["👍", "✓", "👀"];
const FOLD_LINES = 20;
const REPLY_LABEL_CHARS = 40;
const TODO_TOOL = "TodoWrite";
const SUGGEST_TOOL = "mcp__aurelius__suggest_replies";

// ---- Grouping and separators ----------------------------------------------------

export function authorKey(m: MessageView): string | null {
  if (m.system) return null;
  if (m.author_user) return `member:${m.author_user.id}`;
  if (m.role === "user") return "member";
  if (m.update && m.update !== "group") return null;
  const name = m.author?.trim();
  return name ? `employee:${name}` : null;
}

export const localDay = (ms: number, offsetMinutes: number) => Math.floor((ms + offsetMinutes * 60_000) / DAY_MS);

export function clock(ms: number, offsetMinutes: number): string {
  const minutes = ((Math.floor((ms + offsetMinutes * 60_000) / 60_000) % 1440) + 1440) % 1440;
  return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

export function dayLabel(day: number, today: number): string {
  if (today === day) return "Today";
  if (today - day === 1) return "Yesterday";
  const date = new Date(day * DAY_MS);
  const weekday = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][date.getUTCDay()];
  const month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][
    date.getUTCMonth()
  ];
  const label = `${weekday} ${date.getUTCDate()} ${month}`;
  return date.getUTCFullYear() === new Date(today * DAY_MS).getUTCFullYear()
    ? label
    : `${label} ${date.getUTCFullYear()}`;
}

export interface Before {
  day: boolean;
  new: boolean;
  grouped: boolean;
}

export function before(
  messages: MessageView[],
  i: number,
  newAt: string | null,
  dayOf: (ms: number) => number,
): Before {
  const m = messages[i];
  if (!m) return { day: false, new: false, grouped: false };
  const previous = messages[i - 1];
  const day = !previous || dayOf(previous.created_at_ms) !== dayOf(m.created_at_ms);
  const isNew = newAt === m.id;
  let grouped = false;
  if (!day && !isNew && previous) {
    const gap = m.created_at_ms - previous.created_at_ms;
    const key = authorKey(m);
    grouped = key !== null && authorKey(previous) === key && gap >= 0 && gap < GROUP_WINDOW_MS;
  }
  return { day, new: isNew, grouped };
}

/** Index of the first new message; decided once, when a channel opens. */
export function newMessagesAt(
  ordinals: number[],
  readThrough: number | null | undefined,
  unread: number,
): number | null {
  if (unread === 0) return null;
  if (readThrough !== null && readThrough !== undefined) {
    const at = ordinals.findIndex((o) => o > readThrough);
    return at < 0 ? null : at;
  }
  return unread <= ordinals.length ? ordinals.length - unread : null;
}

// ---- Who wrote it ---------------------------------------------------------------

export interface Author {
  name: string;
  kind: string;
  human: boolean;
}

/**
 * `speaker`: who an unsigned answer is from, in a chat with one employee. `me`: the signed-in
 * board member, who wrote a message that names nobody (as on mobile).
 */
export function authorOf(
  m: MessageView,
  roleOf: (name: string) => string | null,
  speaker?: string | null,
  me?: string | null,
): Author {
  if (m.author_user) return { name: m.author_user.username, kind: "board member", human: true };
  if (m.role === "user") return { name: me?.trim() || "You", kind: "board member", human: true };
  const name = m.author?.trim() || speaker?.trim() || "Aurelius";
  return { name, kind: roleOf(name) ?? "employee", human: false };
}

export function isSystemLine(m: MessageView, inGroup: boolean): boolean {
  if (m.role !== "assistant" || !m.update) return false;
  // The server's own lines: no employee wrote them.
  if (m.update === "system" || m.update === "compacted") return true;
  if (m.update === "status") return inGroup;
  return m.update === "group" && !m.author?.trim() && m.status !== "partial";
}

// ---- Mentions -------------------------------------------------------------------

export interface Mentionable {
  name: string;
  kind: string;
  employee: boolean;
}

const nameChar = /[\p{L}\p{N}_.-]/u;
const openBefore = (c: string | undefined) => c === undefined || /\s/.test(c) || c === "(";

export function typingMention(text: string): { at: number; word: string } | null {
  const at = text.lastIndexOf("@");
  if (at < 0 || !openBefore(text[at - 1])) return null;
  const word = text.slice(at + 1);
  return /\s/.test(word) ? null : { at, word };
}

/** `@Everyone` wakes the whole discussion (feature `everyone_mention`); it is not a person. */
export const EVERYONE = "Everyone";
export const EVERYONE_KIND = "wakes everyone here";
const isEveryone = (name: string) => name.toLowerCase() === EVERYONE.toLowerCase();

export function mentionSuggestions(
  text: string,
  people: Mentionable[],
  everyone = false,
): { at: number; found: Mentionable[] } | null {
  const typing = typingMention(text);
  if (!typing) return null;
  const word = typing.word.toLowerCase();
  const all = everyone ? [{ name: EVERYONE, kind: EVERYONE_KIND, employee: false }, ...people] : people;
  if (all.some((p) => p.name.toLowerCase() === word)) return null;
  const found = all.filter((p) => p.name.toLowerCase().startsWith(word));
  return found.length > 0 ? { at: typing.at, found } : null;
}

export const completeMention = (text: string, at: number, name: string) => `${text.slice(0, at)}@${name} `;

function nameAt(text: string, from: number): string {
  let end = from;
  while (end < text.length && nameChar.test(text[end] as string)) end++;
  return text.slice(from, end).replace(/\.+$/, "");
}

/** The `@names` in a text, as written, in order and once each. */
export function mentions(text: string): string[] {
  const found: string[] = [];
  for (let i = text.indexOf("@"); i >= 0; i = text.indexOf("@", i + 1)) {
    if (!openBefore(text[i - 1])) continue;
    const name = nameAt(text, i + 1);
    if (name && !found.some((f) => f.toLowerCase() === name.toLowerCase())) found.push(name);
  }
  return found;
}

/** Whether the text mentions `@Everyone` (any case, whole word, same edges as any `@name`). */
export const mentionsEveryone = (text: string) => mentions(text).some(isEveryone);

export function unknownMentions(text: string, people: Mentionable[], archived: string[], everyone = false): string[] {
  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  return mentions(text)
    .filter((n) => !(everyone && isEveryone(n)))
    .filter((n) => archived.some((a) => same(a, n)) || !people.some((p) => same(p.name, n)));
}

/** The line under the composer for `@Everyone`: whom the message wakes and tells. Without `coordinator`: an older server, which wakes them all. */
export function audienceLine(a: {
  employees: number;
  paused: number;
  board_members: number;
  coordinator?: string | null;
}): string {
  const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
  if (a.coordinator) {
    const paused = a.paused > 0 ? ` (${a.paused} paused)` : "";
    const told =
      a.board_members > 0
        ? `, ${plural(a.board_members, "board member")} ${a.board_members === 1 ? "is" : "are"} told`
        : "";
    return `${a.coordinator} picks who follows up; ${plural(a.employees, "employee")} can read along${paused}${told}`;
  }
  const woken = a.employees === 0 ? "Wakes no employees" : `Wakes ${plural(a.employees, "employee")}`;
  const paused = a.paused > 0 ? ` (${a.paused} paused)` : "";
  const board = a.board_members > 0 ? ` and tells ${plural(a.board_members, "board member")}` : "";
  return `${woken}${paused}${board}`;
}

export function unknownWarning(names: string[]): string | null {
  if (names.length === 0) return null;
  if (names.length === 1) return `Nobody active is called @${names[0]}: it wakes no one.`;
  return `Nobody active is called ${names.map((n) => `@${n}`).join(", ")}: they wake no one.`;
}

/** Known `@names` as links the Markdown shows as pills; yours in bold. Not in code, fences or links. */
export function markMentions(text: string, people: string[], me: string | null): string {
  let out = "";
  let fence = false;
  for (const line of text.match(/[^\n]*\n|[^\n]+$/g) ?? []) {
    const start = line.trimStart();
    const fenced = start.startsWith("```") || start.startsWith("~~~");
    if (fenced || fence) {
      if (fenced) fence = !fence;
      out += line;
      continue;
    }
    let code = false;
    let bracket = 0;
    let previous: string | undefined;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i] as string;
      if (ch === "`") code = !code;
      else if (ch === "[" && !code) bracket++;
      else if (ch === "]" && !code) bracket = Math.max(0, bracket - 1);
      else if (ch === "@" && !code && bracket === 0 && openBefore(previous)) {
        const name = nameAt(line, i + 1);
        const known = name ? people.find((p) => p.toLowerCase() === name.toLowerCase()) : undefined;
        if (known) {
          const link = `[@${name}](#@${name})`;
          out += me?.toLowerCase() === known.toLowerCase() ? `**${link}**` : link;
          i += name.length;
          previous = name.at(-1);
          continue;
        }
      }
      out += ch;
      previous = ch;
    }
  }
  return out;
}

// ---- Long text ------------------------------------------------------------------

function fenceOf(line: string): { indent: string; ch: string; len: number; rest: string } | null {
  const body = line.trimStart();
  const ch = body[0];
  if (ch !== "`" && ch !== "~") return null;
  let len = 0;
  while (body[len] === ch) len++;
  return len >= 3 ? { indent: line.slice(0, line.length - body.length), ch, len, rest: body.slice(len) } : null;
}

/** A message past 20 lines cut to them, a code block cut open closed again. `cut` is a UTF-16 offset. */
export function fold(raw: string): { shown: string; cut: number; hidden: number } | null {
  const text = raw.trimEnd();
  const lines = text.split(/(?<=\n)/);
  if (lines.length <= FOLD_LINES) return null;
  let pos = 0;
  let open: { indent: string; ch: string; len: number } | null = null;
  let cutAt = 0;
  let openAtCut: { indent: string; ch: string; len: number } | null = null;
  lines.forEach((line, i) => {
    pos += line.length;
    const f = fenceOf(line.replace(/[\r\n]+$/, ""));
    if (f) {
      if (!open) {
        if (!(f.ch === "`" && f.rest.includes("`"))) open = { indent: f.indent, ch: f.ch, len: f.len };
      } else if (f.ch === open.ch && f.len >= open.len && f.rest.trim() === "") {
        open = null;
      }
    }
    if (i + 1 === FOLD_LINES) {
      cutAt = pos;
      openAtCut = open;
    }
  });
  let shown = text.slice(0, cutAt).trimEnd();
  const reopen = openAtCut as { indent: string; ch: string; len: number } | null;
  if (reopen) shown += `\n${reopen.indent}${reopen.ch.repeat(reopen.len)}`;
  return { shown, cut: cutAt, hidden: lines.length - FOLD_LINES };
}

/** Blocks of Markdown split on blank lines outside code: while streaming only the last one changes. */
export function splitBlocks(text: string): string[] {
  const blocks: string[] = [];
  let current = "";
  let open: { ch: string; len: number } | null = null;
  for (const line of text.split(/(?<=\n)/)) {
    const f = fenceOf(line.replace(/[\r\n]+$/, ""));
    if (f) {
      if (!open) open = { ch: f.ch, len: f.len };
      else if (f.ch === open.ch && f.len >= open.len && f.rest.trim() === "") open = null;
    }
    if (!open && line.trim() === "" && current.trim() !== "") {
      blocks.push(current);
      current = "";
    } else {
      current += line;
    }
  }
  if (current.trim() !== "") blocks.push(current);
  return blocks;
}

export function planTitle(text: string): string {
  for (const line of text.split("\n")) {
    const title = line.trim().replace(/^#+/, "").trim();
    if (title) return title;
  }
  return "Plan";
}

export function webSearchSummary(output: string): string {
  const start = output.indexOf("Links: [");
  if (start < 0) return output;
  const json = output.slice(start + "Links: ".length);
  const end = json.indexOf("}]");
  if (end < 0) return output;
  try {
    const links = JSON.parse(json.slice(0, end + 2)) as { url?: string; title?: string }[];
    if (links.length === 0) return output;
    return links
      .map((l) => {
        const url = l.url ?? "";
        const site = (url.split("/")[2] ?? url).replace(/^www\./, "");
        return `- [${(l.title ?? url).replace(/[[\]]/g, "")}](<${url}>) · ${site}`;
      })
      .join("\n");
  } catch {
    return output;
  }
}

// ---- Channels -------------------------------------------------------------------

const kindOf = (c: ChannelView) => c.kind ?? "general";
const byActivity = (a: ChannelView, b: ChannelView) =>
  (b.last_activity_ms ?? 0) - (a.last_activity_ms ?? 0) || (b.ticket_number ?? 0) - (a.ticket_number ?? 0);
const byName = (a: ChannelView, b: ChannelView) => a.name.toLowerCase().localeCompare(b.name.toLowerCase());

export interface Sections {
  general: ChannelView | null;
  projects: ChannelView[];
  tickets: ChannelView[];
  archived: ChannelView[];
}

export function sections(channels: ChannelView[]): Sections {
  const of = (kind: string, archived: boolean) => channels.filter((c) => kindOf(c) === kind && c.archived === archived);
  return {
    general: channels.find((c) => kindOf(c) === "general") ?? null,
    projects: of("project", false).sort(byName),
    tickets: of("ticket", false).sort(byActivity),
    archived: [...of("project", true).sort(byName), ...of("ticket", true).sort(byActivity)],
  };
}

/** `list` in the frozen order, the ones not in it after: a row never jumps under the pointer. */
export function keepOrder(list: ChannelView[], frozen: string[]): ChannelView[] {
  const place = (c: ChannelView) => {
    const at = frozen.indexOf(c.conversation_id);
    return at < 0 ? Number.MAX_SAFE_INTEGER : at;
  };
  return [...list].sort((a, b) => place(a) - place(b));
}

export const shownCount = (len: number, expanded: boolean) => (expanded ? len : Math.min(len, SHOW_MORE_AFTER));

export function visibleChannels(
  s: Sections,
  ticketsAll: boolean,
  archivedOpen: boolean,
  archivedAll: boolean,
): ChannelView[] {
  return [
    ...(s.general ? [s.general] : []),
    ...s.projects,
    ...s.tickets.slice(0, shownCount(s.tickets.length, ticketsAll)),
    ...s.archived.slice(0, archivedOpen ? shownCount(s.archived.length, archivedAll) : 0),
  ];
}

export function bareName(c: ChannelView): string {
  const kind = kindOf(c);
  if (kind === "general") return "general";
  return kind === "ticket" ? c.name.replace(/^#+/, "") : c.name;
}

export function channelLabel(c: ChannelView): string {
  let name = bareName(c);
  if (kindOf(c) === "ticket" && c.ticket_number !== null && c.ticket_number !== undefined) {
    const n = String(c.ticket_number);
    const slug = name.startsWith(n) ? name.slice(n.length).replace(/^-+/, "") : name;
    name = slug ? `Ticket ${n}, ${slug}` : `Ticket ${n}`;
  }
  const parts = [name];
  if (c.unread > 0) parts.push(`${c.unread} unread`);
  if (c.mentions > 0) parts.push(`${c.mentions} mention${c.mentions === 1 ? "" : "s"}`);
  if (c.archived) parts.push("archived, read-only");
  return parts.join(", ");
}

export function chatBadge(channels: ChannelView[]): { mentions: number; unread: boolean } {
  return { mentions: channels.reduce((n, c) => n + c.mentions, 0), unread: channels.some((c) => c.unread > 0) };
}

export function stepChannel(visible: string[], current: string | null, down: boolean): string | null {
  if (visible.length === 0) return null;
  const at = Math.max(0, current === null ? 0 : visible.indexOf(current));
  return visible[down ? Math.min(at + 1, visible.length - 1) : Math.max(at - 1, 0)] ?? null;
}

export function readOnlyBar(c: ChannelView, ticketDone: boolean): string {
  if (c.ticket_number !== null && c.ticket_number !== undefined) {
    return `#${c.ticket_number} is ${ticketDone ? "Done" : "archived"} · read-only.`;
  }
  return "This project is archived · read-only.";
}

// ---- Threads and reactions ------------------------------------------------------

export interface Caps {
  threads: boolean;
  reactions: boolean;
  readOnly: boolean;
}

export function rowActions(m: MessageView, caps: Caps): { react: boolean; reply: boolean } {
  const open = !caps.readOnly && !m.system && m.status !== "partial" && !m.queued && !m.deleted_at_ms;
  return { react: open && caps.reactions, reply: open && caps.threads && !m.thread_parent };
}

export const replies = (n: number) => (n === 1 ? "1 reply" : `${n} replies`);

function lastReply(ms: number, offset: number, today: number): string {
  const day = localDay(ms, offset);
  return day === today ? clock(ms, offset) : dayLabel(day, today);
}

export function threadLine(m: MessageView, offset: number, today: number): string | null {
  const count = m.thread_reply_count ?? 0;
  if (count <= 0) return null;
  return m.thread_last_reply_ms
    ? `↳ ${replies(count)} · last ${lastReply(m.thread_last_reply_ms, offset, today)}`
    : `↳ ${replies(count)}`;
}

export function threadLabel(m: MessageView, offset: number, today: number): string {
  let label = `Thread, ${replies(m.thread_reply_count ?? 0)}`;
  if ((m.thread_unread ?? 0) > 0) label += `, ${m.thread_unread} unread`;
  if (m.thread_last_reply_ms) label += `, last reply ${lastReply(m.thread_last_reply_ms, offset, today)}`;
  return label;
}

export type StreamTo = "channel" | "thread" | "elsewhere";

export function streamTo(threadParent: string | null | undefined, openThread: string | null): StreamTo {
  if (!threadParent) return "channel";
  return threadParent === openThread ? "thread" : "elsewhere";
}

export const reactionOn = (reactions: ReactionView[], emoji: string) =>
  !reactions.some((r) => r.emoji === emoji && r.mine);

export function applyReaction(
  reactions: ReactionView[],
  emoji: string,
  on: boolean,
  me: string | null,
): ReactionView[] {
  const at = reactions.findIndex((r) => r.emoji === emoji);
  const found = reactions[at];
  if (on && !found) return [...reactions, { emoji, count: 1, mine: true, by: me ? [me] : [] }];
  if (on && found && !found.mine) {
    return reactions.map((r, i) =>
      i === at ? { ...r, count: r.count + 1, mine: true, by: me ? [...(r.by ?? []), me] : r.by } : r,
    );
  }
  if (!on && found?.mine) {
    if (found.count <= 1) return reactions.filter((_, i) => i !== at);
    const by = [...(found.by ?? [])];
    const mine = me ? by.lastIndexOf(me) : -1;
    if (mine >= 0) by.splice(mine, 1);
    return reactions.map((r, i) => (i === at ? { ...r, count: r.count - 1, mine: false, by } : r));
  }
  return reactions;
}

export function reactionTip(r: ReactionView): string {
  const by = r.by ?? [];
  return by.length > 0 ? by.join(", ") : `${r.count}`;
}

// ---- Progress -------------------------------------------------------------------

export type TodoState = "done" | "doing" | "open";

export interface Todo {
  text: string;
  state: TodoState;
}

const latest = (activity: ActivityView[], tool: string) =>
  activity
    .filter((a) => a.tool === tool)
    .reduce<ActivityView | null>((best, a) => (!best || a.offset >= best.offset ? a : best), null);

export function quickReplies(activity: ActivityView[]): { question: string | null; options: string[] } | null {
  const input = latest(activity, SUGGEST_TOOL)?.input as { options?: unknown; question?: unknown } | undefined;
  if (!Array.isArray(input?.options)) return null;
  const options = (input.options as unknown[])
    .filter((o): o is string => typeof o === "string")
    .map((o) => o.trim())
    .filter(Boolean)
    .slice(0, 3);
  if (options.length < 2) return null;
  const question = typeof input.question === "string" ? input.question.trim() : "";
  return { question: question || null, options };
}

export function replyLabel(reply: string): string {
  const chars = [...reply];
  return chars.length <= REPLY_LABEL_CHARS
    ? reply
    : `${chars
        .slice(0, REPLY_LABEL_CHARS - 1)
        .join("")
        .trimEnd()}…`;
}

/** Claude Code's own todo list: `TodoWrite` before CLI 2.1.318, the Task* tools since. */
const TODO_TOOLS = new Set([TODO_TOOL, "TaskCreate", "TaskUpdate", "TaskList", "TaskGet"]);

export const isProgressTool = (tool: string) => TODO_TOOLS.has(tool) || tool === SUGGEST_TOOL;

function kind(tool: string): [string, string, string] {
  switch (tool) {
    case "Bash":
    case "PowerShell":
      return ["Ran a command", "Ran {n} commands", "Running a command"];
    case "Read":
      return ["Read a file", "Read {n} files", "Reading a file"];
    case "Edit":
    case "MultiEdit":
    case "Write":
    case "NotebookEdit":
      return ["Edited a file", "Edited {n} files", "Editing a file"];
    case "Glob":
    case "Grep":
      return ["Searched the files", "Searched the files {n} times", "Searching the files"];
    case "WebSearch":
      return ["Searched the web", "Searched the web {n} times", "Searching the web"];
    case "WebFetch":
      return ["Opened a page", "Opened {n} pages", "Opening a page"];
    case "Task":
    case "Agent":
      return ["Ran a subagent", "Ran {n} subagents", "Running a subagent"];
    default:
      return ["Ran a tool", "Ran {n} tools", "Running a tool"];
  }
}

export function runLabel(run: ActivityView[]): string {
  const last = run.at(-1);
  if (!last) return "";
  const open = [...run].reverse().find((a) => a.ok === null || a.ok === undefined);
  if (open) return kind(open.tool)[2];
  const same = run.every((a) => kind(a.tool)[0] === kind(last.tool)[0]);
  const failed = run.filter((a) => a.ok === false).length;
  const base = same
    ? run.length === 1
      ? kind(last.tool)[0]
      : kind(last.tool)[1].replace("{n}", String(run.length))
    : `Ran ${run.length} steps`;
  return failed > 0 ? `${base} · ${failed} failed` : base;
}

// ---- System lines ---------------------------------------------------------------

const or = (v: string | null | undefined, d: string) => v?.trim() || d;

export function systemText(line: SystemLine, text: string, ticket: number | null): string {
  const given = text.trim();
  if (given) return given;
  const by = or(line.by, "Aurelius");
  const number = ticket === null ? "The ticket" : `#${ticket}`;
  switch (line.action) {
    case "responsible":
      if (line.from && line.to) return `${by} changed responsible: ${line.from} → ${line.to}`;
      if (line.to) return `${by} made ${line.to} responsible`;
      return line.from ? `${by} removed ${line.from} as responsible` : `${by} changed who is responsible`;
    case "opened":
      return `${number} opened · ${by}`;
    case "closed":
      return `${number} closed · ${by}`;
    case "reopened":
      return `${number} reopened · ${by}`;
    case "approved":
      return `${by} approved the plan`;
    case "adjusted":
      return `${by} asked to adjust the plan`;
    case "sent_back":
      return `${by} sent ${ticket === null ? "the ticket" : `#${ticket}`} back`;
    case "tested_passed":
      return "Tests passed";
    case "tested_failed":
      return "Tests failed";
    case "joined":
      return `${by} joined`;
    default:
  }
  switch (line.kind) {
    case "delegated":
      return `${or(line.from, "An employee")} gave a task to ${or(line.to, "a colleague")}`;
    case "reported":
      return `${or(line.from, "A colleague")} reported back on a task`;
    case "moved":
      if (line.from && line.to) return `${by} moved the ticket from ${line.from} to ${line.to}`;
      return line.to ? `${by} moved the ticket to ${line.to}` : `${by} moved the ticket`;
    case "plan":
      return `${or(line.from, "The owner")} handed in a plan. It waits for a board member to approve it`;
    case "review":
      return `${or(line.from, "The owner")} finished the work and handed in the report. It is ready for your review`;
    case "done":
      return `${by} accepted the report and moved the ticket to Done`;
    case "created_from":
      return `Created from a message${line.by ? ` · ${line.by}` : ""}`;
    case "ticket_created":
      return `${by} created a ticket from a message`;
    case "summary":
      return "A summary of this ticket was saved to project memory";
    case "summary_failed":
      return "The summary of this ticket could not be saved to project memory";
    case "limit":
      return `Limit reached: ${or(line.to, "nobody")} was not woken.`;
    default:
      return by;
  }
}

export function lineMark(line: SystemLine, text: string): { mark: string; failed: boolean } {
  const quiet = (mark: string) => ({ mark, failed: false });
  switch (line.action) {
    case "opened":
    case "joined":
      return quiet("+ ");
    case "closed":
      return quiet("■ ");
    case "reopened":
      return quiet("↺ ");
    case "approved":
      return quiet("✓ ");
    case "adjusted":
    case "sent_back":
      return quiet("↩ ");
    case "tested_passed":
      return quiet("▶ ✓ ");
    case "tested_failed":
      return { mark: "▶ ✕ ", failed: true };
    default:
  }
  switch (line.kind) {
    case "moved":
      return quiet("→ ");
    case "done":
      return quiet("■ ");
    case "plan":
    case "review":
      return quiet("◆ ");
    case "delegated":
      return quiet("↳ ");
    case "reported":
      return text.toLowerCase().includes("failed") ? { mark: "▶ ✕ ", failed: true } : quiet("▶ ");
    case "summary_failed":
      return { mark: "", failed: true };
    case "limit": {
      const t = text.trim();
      if (t.startsWith("Paused:")) return quiet("⏸ ");
      if (t === "Resumed.") return quiet("▸ ");
      return t.startsWith("Waiting:") || t.startsWith("Connect your Claude login") ? quiet("◆ ") : quiet("");
    }
    default:
      return quiet("");
  }
}

// ---- A ticket from a message, and editing ---------------------------------------

export function prefill(
  text: string,
  author: string,
  place: string,
  when: string,
): { title: string; description: string } {
  const first =
    text
      .split("\n")
      .map((l) => l.trim().replace(/^#+/, "").trim())
      .find(Boolean) ?? "From a message";
  const chars = [...first];
  const title = chars.length > 120 ? `${chars.slice(0, 119).join("").trimEnd()}…` : first;
  const quoted = text
    .trim()
    .split("\n")
    .map((l) => (l === "" ? ">" : `> ${l}`));
  return { title, description: `${quoted.join("\n")}\n\nFrom ${author} in ${place} · ${when}` };
}

export const canMakeTicket = (m: MessageView) =>
  !m.system && m.status !== "partial" && !m.queued && m.text.trim() !== "";

export type Save = { kind: "off" } | { kind: "close" } | { kind: "send"; text: string };

export function saveOf(original: string, next: string): Save {
  const text = next.trim();
  if (text === "") return { kind: "off" };
  return text === original.trim() ? { kind: "close" } : { kind: "send", text };
}

export const canChange = (m: MessageView, supported: boolean) => supported && Boolean(m.editable) && !m.deleted_at_ms;

// ---- Text between steps ---------------------------------------------------------

/** `ActivityView.offset` counts UTF-8 bytes; a JavaScript string counts UTF-16 units. */
export function indexOfByte(text: string, bytes: number): number {
  let seen = 0;
  let i = 0;
  while (i < text.length && seen < bytes) {
    const code = text.codePointAt(i) as number;
    seen += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    i += code >= 0x10000 ? 2 : 1;
  }
  return i;
}

export type Segment = { kind: "text"; text: string } | { kind: "steps"; steps: ActivityView[] };

/** Who runs `runId`, from the employees' working agents. */
export function runAuthor(
  employees: { name: string; agents?: { run_id?: string | null }[] }[],
  runId: string,
): string | null {
  return employees.find((e) => e.agents?.some((a) => a.run_id === runId))?.name ?? null;
}

export function ownerAnswers(owner: string | null): string {
  return owner ? `; without a name, ${owner} (owner) answers` : "; without a name, no one is woken";
}

/** The ticket already made from a message, from the channel's `ticket_created` lines. */
export function ticketFrom(list: MessageView[], id: string): { ticket: string | null; label: string } | null {
  for (let i = list.length - 1; i >= 0; i--) {
    const m = list[i] as MessageView;
    if (m.system?.kind === "ticket_created" && m.system.message_id === id) {
      const number = /#(\d+)/.exec(m.text)?.[1];
      return { ticket: m.system.ticket_id ?? null, label: number ? `#${number}` : "a ticket" };
    }
  }
  return null;
}

/** Whether a text calls this person by name. */
export function mentionsMe(text: string, me: string | null): boolean {
  if (!me) return false;
  const name = me.toLowerCase();
  return mentions(text).some((m) => m.toLowerCase() === name);
}

/**
 * The line under an answer that is still being written. Compacting comes first: while
 * the context is summarised nothing else moves, and it can take minutes.
 */
export function statusLine(text: string, waiting: boolean, compacting: boolean): string {
  if (waiting) return "Waiting for your approval";
  if (compacting) return "Compacting context…";
  return text ? "Writing…" : "Thinking…";
}
