// The parts of the ticket page that read the ticket's tasks (tickets #74, #76 and #83): what each
// task's conversation says right now, read once for all tasks of the ticket, and the Timeline's
// filter and work blocks. The Activity tab with its Plan and employee cards is ticket-activity.tsx.

import type { ActivityView, MessageView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useMessagesOf } from "../data";
import { initial, Mark, markColor } from "../ui/marks";
import { useLiveAll, useLiveStoreOf } from "./chat-live";
import { clock, type Todo } from "./chat-logic";
import { stepLine } from "./tasks-logic";
import {
  factsOf,
  stepKind,
  stepResult,
  type TaskWork,
  type WorkBlock,
  workBlocks,
} from "./ticket-activity-plain-logic";
import { latestTodos } from "./ticket-overview-logic";
import { ACTIVITY_FILTERS, type ActivityFilter } from "./ticket-page-logic";

export const pageStyles = stylex.create({
  // Fact chips, shared by the Timeline's blocks and the Activity's cards.
  facts: { display: "flex", flexWrap: "wrap", gap: 6, margin: 0, padding: 0, listStyle: "none" },
  fact: {
    borderRadius: 999,
    paddingInline: 8,
    paddingBlock: 2,
    fontSize: 11.5,
    whiteSpace: "nowrap",
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--color-neutral)",
  },
  factGood: { color: "var(--color-text-success)", backgroundColor: "var(--color-success-muted)" },
  factBad: { color: "var(--color-text-error)", backgroundColor: "var(--color-error-muted)" },
  link: {
    borderWidth: 0,
    borderRadius: 5,
    paddingInline: 6,
    paddingBlock: 3,
    cursor: "pointer",
    fontSize: 12,
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    backgroundColor: { default: "transparent", ":hover": "var(--color-neutral)" },
  },
});

const styles = stylex.create({
  section: { display: "flex", flexDirection: "column", gap: 8, paddingTop: 16 },
  heading: { margin: 0, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  // The Timeline's head: what to show, whose, and the order.
  head: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8, paddingBlock: 6 },
  grow: { flexGrow: 1 },
  seg: {
    display: "inline-flex",
    margin: 0,
    gap: 2,
    padding: 2,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: 7,
  },
  segButton: {
    borderWidth: 0,
    borderRadius: 5,
    paddingInline: 10,
    paddingBlock: 2,
    cursor: "pointer",
    fontSize: 12,
    color: "var(--color-text-secondary)",
    backgroundColor: "transparent",
  },
  segOn: { color: "var(--color-text-primary)", backgroundColor: "var(--color-neutral)" },
  people: { display: "inline-flex", flexWrap: "wrap", gap: 4 },
  person: {
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: 999,
    paddingBlock: 1,
    paddingInlineStart: 3,
    paddingInlineEnd: 9,
    cursor: "pointer",
    fontSize: 12,
    color: "var(--color-text-secondary)",
    backgroundColor: "transparent",
  },
  personOn: {
    borderColor: "var(--color-text-accent)",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-neutral)",
  },
  order: { fontSize: 12, color: "var(--precision-text3)" },
  // A work block of the Timeline.
  ev: { display: "flex", gap: 10, paddingBlock: 5 },
  body: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column", gap: 4 },
  who: { fontSize: 12.5, fontWeight: 600, color: "var(--color-text-primary)" },
  time: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)", fontVariantNumeric: "tabular-nums" },
  work: {
    display: "flex",
    flexDirection: "column",
    gap: 2,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 10,
    paddingInline: 12,
    paddingBlock: 8,
    backgroundColor: "var(--color-background-card)",
  },
  workLive: { borderColor: "color-mix(in srgb, var(--color-text-success) 35%, transparent)" },
  title: { display: "flex", alignItems: "center", gap: 6, fontWeight: 600, color: "var(--color-text-primary)" },
  glyphDone: { color: "var(--color-text-success)" },
  glyphLive: { color: "var(--color-text-accent)" },
  glyphOpen: { color: "var(--precision-text3)" },
  note: {
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
    display: "-webkit-box",
    WebkitBoxOrient: "vertical",
    WebkitLineClamp: 3,
    overflow: "hidden",
  },
  blockFacts: { paddingTop: 4 },
  toggle: { alignSelf: "flex-start", marginTop: 2, marginInlineStart: -6 },
  steps: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    margin: 0,
    marginTop: 6,
    paddingTop: 6,
    paddingInlineStart: 0,
    listStyle: "none",
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    fontSize: 12,
  },
  step: { display: "flex", alignItems: "baseline", gap: 8, minWidth: 0, color: "var(--color-text-secondary)" },
  stepKind: { width: 70, flexShrink: 0, color: "var(--precision-text3)" },
  code: {
    minWidth: 0,
    borderRadius: 4,
    paddingInline: 5,
    fontFamily: "var(--font-family-code)",
    fontSize: 11.5,
    overflowWrap: "anywhere",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-neutral)",
  },
  result: { marginInlineStart: "auto", flexShrink: 0, whiteSpace: "nowrap", color: "var(--precision-text3)" },
  resultGood: { color: "var(--color-text-success)" },
  resultBad: { color: "var(--color-text-error)" },
});

/** Every second while `on`, so a timer moves. */
export function useNow(on: boolean, everyMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), everyMs);
    return () => clearInterval(timer);
  }, [on, everyMs]);
  return now;
}

/** What one task's conversation says right now. */
export interface TaskLive {
  todos: Todo[] | null;
  /** Something streams or a partial answer is open: the run is at work this moment. */
  streaming: boolean;
  /** The employee's latest answer, streaming or not: its text and its calls. */
  latest: { text: string; activity: ActivityView[] } | null;
  /** Every call of the conversation, for the task's facts. */
  all: ActivityView[];
  /** The runs of the conversation, to know which posts in the discussion are this task's. */
  runIds: Set<string>;
}

type LiveUpdate = ReturnType<typeof useLiveAll>[number];

const EMPTY: TaskLive = { todos: null, streaming: false, latest: null, all: [], runIds: new Set() };

function taskLive(messages: readonly MessageView[], live: readonly LiveUpdate[]): TaskLive {
  const liveOf = new Map(live.map((u) => [u.message_id, u]));
  const answers = messages.filter((m) => m.role === "assistant" && m.run_id && !m.update);
  const known = new Set(answers.map((m) => m.id));
  const fresh = live.filter((u) => !known.has(u.message_id));
  const all = [
    ...answers.flatMap((m) => liveOf.get(m.id)?.activity ?? m.activity),
    ...fresh.flatMap((u) => u.activity),
  ];
  const newest = live.at(-1) ?? null;
  const last = answers.at(-1);
  const latest = newest
    ? { text: newest.text, activity: newest.activity }
    : last
      ? { text: last.text, activity: last.activity }
      : null;
  return {
    todos: latestTodos(
      [...live].reverse().map((u) => u.activity),
      [...messages],
    ),
    streaming: newest !== null || messages.some((m) => m.status === "partial"),
    latest,
    all,
    runIds: new Set(messages.flatMap((m) => m.run_id ?? [])),
  };
}

/** What the ticket's tasks do: each task's state of the moment, and their work as Timeline blocks. */
export interface TicketWork {
  live: (taskId: string) => TaskLive;
  blocks: WorkBlock[];
}

/**
 * Every task's conversation of the ticket, read once under the same keys as its run screen, with
 * what streams in them now: the Activity's cards and the Timeline's blocks both come from it.
 */
export function useTicketWork(tasks: readonly TaskView[]): TicketWork {
  const ids = useMemo(() => tasks.flatMap((t) => t.conversation_id ?? []), [tasks]);
  const reads = useMessagesOf(ids);
  const store = useLiveStoreOf(ids);
  const live = useLiveAll(store);
  const lists = reads.map((r) => r.data?.messages);
  // useQueries gives a new array every render; what each read holds is what counts.
  const stamp = reads.map((r) => r.dataUpdatedAt).join(" ");
  // biome-ignore lint/correctness/useExhaustiveDependencies: `stamp` says when `lists` changed.
  useEffect(() => {
    store.settle(
      new Set(
        lists
          .flatMap((l) => l ?? [])
          .filter((m) => m.status !== "partial")
          .map((m) => m.id),
      ),
    );
  }, [stamp, store]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `stamp` says when `lists` changed.
  return useMemo(() => {
    const byConversation = new Map(ids.map((id, i) => [id, lists[i] ?? []]));
    const byTask = new Map<string, TaskLive>();
    const work: TaskWork[] = [];
    for (const t of tasks) {
      if (!t.conversation_id) continue;
      const messages = byConversation.get(t.conversation_id) ?? [];
      const own = live.filter((u) => u.conversation_id === t.conversation_id);
      byTask.set(t.id, taskLive(messages, own));
      work.push({ taskId: t.id, who: t.employee_name ?? null, messages, live: own });
    }
    return { live: (id: string) => byTask.get(id) ?? EMPTY, blocks: workBlocks(work) };
  }, [stamp, ids, tasks, live]);
}

/** A section of the page's main column with a heading. */
export function PageSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title} {...stylex.props(styles.section)}>
      <h2 {...stylex.props(styles.heading)}>{title}</h2>
      {children}
    </section>
  );
}

/** One employee of the Timeline's filter. */
export interface Person {
  name: string;
}

/** The Timeline's head: All / Changes / Messages, one chip per employee, and the order it reads in. */
export function ActivityHead({
  filter,
  onFilter,
  people = [],
  who = null,
  onWho,
  newestFirst = false,
}: {
  filter: ActivityFilter;
  onFilter: (f: ActivityFilter) => void;
  people?: readonly Person[];
  who?: string | null;
  onWho?: (who: string | null) => void;
  newestFirst?: boolean;
}) {
  return (
    <div {...stylex.props(styles.head)}>
      <fieldset aria-label="Show" {...stylex.props(styles.seg)}>
        {ACTIVITY_FILTERS.map((f) => (
          <button
            key={f.key}
            type="button"
            aria-pressed={filter === f.key}
            onClick={() => onFilter(f.key)}
            {...stylex.props(styles.segButton, filter === f.key && styles.segOn)}
          >
            {f.label}
          </button>
        ))}
      </fieldset>
      {people.length > 0 && onWho ? (
        <fieldset aria-label="Employee" {...stylex.props(styles.seg, styles.people)} style={{ borderWidth: 0 }}>
          {people.map((p) => (
            <button
              key={p.name}
              type="button"
              aria-pressed={who === p.name}
              onClick={() => onWho(who === p.name ? null : p.name)}
              {...stylex.props(styles.person, who === p.name && styles.personOn)}
            >
              <Mark letter={initial(p.name)} color={markColor(p.name)} size={16} radius={4} />
              {p.name}
            </button>
          ))}
        </fieldset>
      ) : null}
      <span {...stylex.props(styles.grow)} />
      {newestFirst ? <span {...stylex.props(styles.order)}>Newest first</span> : null}
    </div>
  );
}

/** Fact chips: "Changed 3 files", "Tests pass". */
export function Facts({ steps, withLines = false }: { steps: readonly ActivityView[]; withLines?: boolean }) {
  const facts = factsOf(steps, withLines);
  if (facts.length === 0) return null;
  return (
    <ul aria-label="What it did" {...stylex.props(pageStyles.facts)}>
      {facts.map((f) => (
        <li
          key={f.text}
          {...stylex.props(
            pageStyles.fact,
            f.tone === "good" && pageStyles.factGood,
            f.tone === "bad" && pageStyles.factBad,
          )}
        >
          {f.text}
        </li>
      ))}
    </ul>
  );
}

const zone = () => -new Date().getTimezoneOffset();

/** "10:44", "10:44–10:55", or "now" for what still streams. */
function blockTime(b: WorkBlock): string {
  if (!Number.isFinite(b.atMs)) return "now";
  const from = clock(b.atMs, zone());
  const to = Number.isFinite(b.endMs) ? clock(b.endMs, zone()) : "now";
  return to === from ? from : `${from}–${to}`;
}

/**
 * One block of the Timeline (ticket #83): an employee's work on one step of its plan, with its
 * own words and a few facts; "Show the N steps" opens what it ran and changed.
 */
export function WorkBlockItem({ block }: { block: WorkBlock }) {
  const [open, setOpen] = useState(false);
  const live = block.state === "live";
  return (
    <article aria-label={block.who ? `${block.who} · ${block.title}` : block.title} {...stylex.props(styles.ev)}>
      {block.who ? <Mark letter={initial(block.who)} color={markColor(block.who)} size={28} radius={8} /> : null}
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.who)}>
          {block.who ?? "An employee"} <span {...stylex.props(styles.time)}>{blockTime(block)}</span>
        </div>
        <div {...stylex.props(styles.work, live && styles.workLive)}>
          <div {...stylex.props(styles.title)}>
            <span
              aria-hidden="true"
              {...stylex.props(block.state === "done" ? styles.glyphDone : live ? styles.glyphLive : styles.glyphOpen)}
            >
              {block.state === "done" ? "✓" : live ? "●" : "○"}
            </span>
            {block.title}
          </div>
          {block.note ? <div {...stylex.props(styles.note)}>{block.note}</div> : null}
          <div {...stylex.props(styles.blockFacts)}>
            <Facts steps={block.steps} withLines />
          </div>
          {block.steps.length > 0 ? (
            <button
              type="button"
              aria-expanded={open}
              onClick={() => setOpen(!open)}
              {...stylex.props(pageStyles.link, styles.toggle)}
            >
              {`${open ? "⌄ Hide" : "› Show"} the ${block.steps.length === 1 ? "step" : `${block.steps.length} steps`}`}
            </button>
          ) : null}
          {open ? (
            <ul aria-label="Steps" {...stylex.props(styles.steps)}>
              {block.steps.map((s) => (
                <BlockStep key={s.call_id} step={s} />
              ))}
            </ul>
          ) : null}
        </div>
      </div>
    </article>
  );
}

function BlockStep({ step }: { step: ActivityView }) {
  const result = stepResult(step);
  return (
    <li {...stylex.props(styles.step)}>
      <span {...stylex.props(styles.stepKind)}>{stepKind(step)}</span>
      <code {...stylex.props(styles.code)}>{stepLine(step)}</code>
      {result ? (
        <span
          {...stylex.props(
            styles.result,
            result.tone === "good" && styles.resultGood,
            result.tone === "bad" && styles.resultBad,
          )}
        >
          {result.text}
        </span>
      ) : null}
    </li>
  );
}
