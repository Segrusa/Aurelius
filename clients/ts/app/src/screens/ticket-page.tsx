// The parts of the ticket page (ticket #74) around the ticket's talk: what every task of the ticket
// does now (a ticket can have a main task and several delegations at once), the main task's plan
// with the delegations below it, and what runs this second above the composer. Each part reads the
// task's own conversation, the same reads the task's run screen uses, so they share the cache.

import { Button } from "@astryxdesign/core/Button";
import type { ActivityView, MessageView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useMessages, useMessagesOf } from "../data";
import { Icon, StatusGlyph } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { useLiveAll, useLiveStore, useLiveStoreOf } from "./chat-live";
import { clock, type Todo } from "./chat-logic";
import { stepLine, taskStateLine, treeOrder } from "./tasks-logic";
import {
  diffCounts,
  fileName,
  rowDiff,
  rowFiles,
  runningTitle,
  type WorkRow,
  workKind,
  workRows,
  workTitle,
} from "./ticket-activity-logic";
import { TodoMark } from "./ticket-overview";
import { elapsed, latestTodos } from "./ticket-overview-logic";
import {
  ACTIVITY_FILTERS,
  type ActivityFilter,
  currentCall,
  lastSteps,
  nowTasks,
  nowTitle,
  planStep,
  progressLine,
  questionsText,
  runActivity,
  runCounts,
} from "./ticket-page-logic";

/** How many of the open step's latest calls the Plan shows. */
const PLAN_STEPS_SHOWN = 3;
/** How many tasks the live bar names at once; the rest are on the Now card. */
const LIVE_BAR_MAX = 3;

const styles = stylex.create({
  section: { display: "flex", flexDirection: "column", gap: 8, paddingTop: 16 },
  heading: { margin: 0, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  card: {
    display: "flex",
    flexDirection: "column",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 10,
    backgroundColor: "var(--color-background-surface)",
  },
  task: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    padding: 14,
    borderTopWidth: { default: 1, ":first-child": 0 },
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border)",
  },
  taskHead: { display: "flex", alignItems: "center", gap: 10, minWidth: 0 },
  taskLines: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column", gap: 2 },
  taskTitle: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontWeight: 600,
    color: "var(--color-text-primary)",
  },
  taskSub: { fontSize: 12, color: "var(--color-text-secondary)" },
  tools: { display: "flex", flexShrink: 0, gap: 4 },
  bar: { display: "flex", gap: 4 },
  segment: { height: 4, flexGrow: 1, flexBasis: 0, borderRadius: 2, backgroundColor: "var(--color-border)" },
  segmentDone: { backgroundColor: "var(--color-text-success)" },
  segmentDoing: { backgroundColor: "var(--color-text-accent)" },
  counts: {
    display: "flex",
    flexWrap: "wrap",
    columnGap: 16,
    rowGap: 4,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  num: { fontWeight: 600, color: "var(--color-text-primary)" },
  plus: { color: "var(--color-text-success)" },
  minus: { color: "var(--color-text-error)" },
  warn: { color: "var(--color-text-warning)" },
  ask: {
    borderWidth: 0,
    padding: 0,
    cursor: "pointer",
    backgroundColor: "transparent",
    fontSize: 12,
    color: "var(--color-text-warning)",
    textDecoration: { default: "none", ":hover": "underline" },
  },
  // The Activity heading with its filter.
  activityHead: { display: "flex", alignItems: "center", gap: 10, paddingTop: 20, paddingBottom: 4 },
  grow: { flexGrow: 1 },
  filters: {
    display: "flex",
    margin: 0,
    borderWidth: 0,
    gap: 2,
    padding: 2,
    borderRadius: 8,
    backgroundColor: "var(--precision-sunk)",
  },
  filter: {
    borderWidth: 0,
    borderRadius: 6,
    paddingInline: 10,
    paddingBlock: 3,
    cursor: "pointer",
    fontSize: 12,
    color: "var(--color-text-secondary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  filterOn: { color: "var(--color-text-primary)", backgroundColor: "var(--color-background-surface)" },
  // The live bar above the composer.
  live: {
    marginInline: "auto",
    display: "flex",
    width: "100%",
    maxWidth: 820,
    boxSizing: "border-box",
    flexShrink: 0,
    flexDirection: "column",
    gap: 2,
    paddingInline: 24,
    paddingBottom: 6,
  },
  liveRow: { display: "flex", alignItems: "center", gap: 8, minHeight: 26, fontSize: 12 },
  pulse: { width: 7, height: 7, flexShrink: 0, borderRadius: "50%", backgroundColor: "var(--color-text-accent)" },
  liveWho: { flexShrink: 0, fontWeight: 600 },
  liveWhat: { flexShrink: 0, color: "var(--color-text-secondary)" },
  code: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    color: "var(--color-text-primary)",
  },
  liveTime: { flexShrink: 0, color: "var(--precision-text3)", fontVariantNumeric: "tabular-nums" },
  linkButton: {
    flexShrink: 0,
    borderWidth: 0,
    padding: 0,
    cursor: "pointer",
    backgroundColor: "transparent",
    fontSize: 12,
    color: "var(--color-text-accent)",
  },
  // The Plan of the details panel.
  plan: { display: "flex", flexDirection: "column", gap: 2 },
  planHead: { display: "flex", alignItems: "baseline", gap: 8, marginBottom: 4 },
  planTitle: { margin: 0, fontSize: 13, fontWeight: 600 },
  planWho: { minWidth: 0, flexGrow: 1, fontSize: 12, color: "var(--color-text-secondary)" },
  planCount: { flexShrink: 0, fontSize: 12, color: "var(--precision-text3)" },
  step: {
    display: "flex",
    width: "100%",
    alignItems: "flex-start",
    gap: 8,
    borderWidth: 0,
    borderRadius: 6,
    paddingBlock: 5,
    paddingInline: 6,
    textAlign: "left",
    fontSize: 13,
    lineHeight: "18px",
    color: "var(--color-text-primary)",
    backgroundColor: "transparent",
  },
  stepButton: {
    cursor: "pointer",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  stepDoing: { fontWeight: 600 },
  stepOpen: { color: "var(--color-text-secondary)" },
  stepDone: { color: "var(--precision-text3)" },
  stepText: { minWidth: 0, flexGrow: 1, flexBasis: 0 },
  chevron: { display: "flex", flexShrink: 0, color: "var(--precision-text3)", transitionDuration: "120ms" },
  chevronShut: { transform: "rotate(-90deg)" },
  calls: {
    display: "flex",
    flexDirection: "column",
    gap: 3,
    margin: 0,
    paddingInlineStart: 30,
    paddingBottom: 6,
    listStyle: "none",
  },
  call: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    minWidth: 0,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  callMark: { display: "flex", flexShrink: 0, width: 12, justifyContent: "center" },
  empty: { margin: 0, fontSize: 12, color: "var(--precision-text3)" },
  subtasks: { display: "flex", flexDirection: "column", gap: 2, marginTop: 12 },
  subHead: { margin: 0, marginBottom: 4, fontSize: 13, fontWeight: 600 },
  sub: { display: "flex", flexDirection: "column" },
  subRow: {
    display: "flex",
    width: "100%",
    alignItems: "center",
    gap: 8,
    minHeight: 28,
    borderWidth: 0,
    borderRadius: 6,
    paddingBlock: 4,
    paddingInline: 6,
    cursor: "pointer",
    textAlign: "left",
    fontSize: 13,
    color: "var(--color-text-primary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  subLines: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column", gap: 1 },
  subTitle: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  subWho: { fontSize: 12, color: "var(--color-text-secondary)" },
  subBody: { display: "flex", flexDirection: "column", gap: 4, paddingInlineStart: 28, paddingBlock: 4 },
  subLink: { fontSize: 12, color: "var(--color-text-accent)", textDecoration: "none" },
  // A row of work in the Activity (ticket #76).
  work: { display: "flex", gap: 10, paddingBlock: 6, paddingInline: 4 },
  workMark: {
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: 6,
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--precision-sunk)",
  },
  workMarkLive: { color: "var(--color-text-accent)" },
  workBody: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column", gap: 6 },
  workHead: { display: "flex", alignItems: "baseline", gap: 8, minHeight: 24, fontSize: 13 },
  workTitle: { color: "var(--color-text-primary)" },
  workWho: { fontWeight: 600, color: "var(--color-text-primary)" },
  workTime: { flexShrink: 0, fontSize: 11, color: "var(--precision-text3)", fontVariantNumeric: "tabular-nums" },
  chips: { display: "flex", flexWrap: "wrap", gap: 4 },
  chip: {
    maxWidth: 260,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    borderRadius: 5,
    paddingInline: 6,
    paddingBlock: 1,
    fontFamily: "var(--font-family-code)",
    fontSize: 11,
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--precision-sunk)",
  },
  box: {
    margin: 0,
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 8,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    lineHeight: "18px",
  },
  diffLine: { paddingInline: 8, whiteSpace: "pre-wrap", overflowWrap: "anywhere" },
  diffFile: {
    paddingBlock: 2,
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--precision-sunk)",
  },
  diffAdd: {
    color: "var(--color-text-success)",
    backgroundColor: "color-mix(in srgb, var(--color-text-success) 8%, transparent)",
  },
  diffDel: {
    color: "var(--color-text-error)",
    backgroundColor: "color-mix(in srgb, var(--color-text-error) 8%, transparent)",
  },
  diffMore: { paddingInline: 8, color: "var(--precision-text3)" },
  command: { paddingInline: 8, paddingBlock: 4, color: "var(--color-text-primary)", overflowWrap: "anywhere" },
  result: {
    margin: 0,
    paddingInline: 8,
    paddingBlock: 4,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border)",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
    color: "var(--color-text-secondary)",
  },
  resultFailed: { color: "var(--color-text-error)" },
  resultMore: { paddingInline: 8, paddingBottom: 4, fontFamily: "inherit" },
  quote: {
    margin: 0,
    paddingInlineStart: 10,
    borderLeftWidth: 3,
    borderLeftStyle: "solid",
    borderLeftColor: "var(--color-border)",
    fontSize: 13,
    color: "var(--color-text-secondary)",
    whiteSpace: "pre-wrap",
  },
  workActions: { display: "flex", gap: 12 },
});

/** How many diff lines and result lines a work row shows before View diff / Show more. */
const DIFF_LINES_SHOWN = 8;
const RESULT_LINES_SHOWN = 6;
const FINDING_CHARS_SHOWN = 600;

/** Every second while `on`, so a timer moves. */
function useNow(on: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [on]);
  return now;
}

/** What one task's conversation says right now: its plan, its running run and what that run did. */
export interface TaskLive {
  todos: Todo[] | null;
  /** The run going on now, or the latest one of a task that waits. */
  runId: string | null;
  /** Something streams or a partial answer is open: the run is at work this moment. */
  streaming: boolean;
  startedAtMs: number | null;
  /** The calls of `runId`. */
  activity: ReturnType<typeof runActivity>;
}

function useTaskLive(task: TaskView): TaskLive {
  const conversation = task.conversation_id ?? null;
  const read = useMessages(conversation);
  const store = useLiveStore(conversation);
  const live = useLiveAll(store);
  const messages: MessageView[] = useMemo(() => read.data?.messages ?? [], [read.data]);
  useEffect(() => {
    store.settle(new Set(messages.filter((m) => m.status !== "partial").map((m) => m.id)));
  }, [messages, store]);
  return useMemo(() => {
    const newest = live.at(-1) ?? null;
    const partial = [...messages].reverse().find((m) => m.status === "partial" && m.run_id) ?? null;
    const last = [...messages].reverse().find((m) => m.role === "assistant" && m.run_id) ?? null;
    const runId = newest?.run_id ?? partial?.run_id ?? last?.run_id ?? null;
    return {
      todos: latestTodos(
        [...live].reverse().map((u) => u.activity),
        messages,
      ),
      runId,
      streaming: newest !== null || partial !== null,
      startedAtMs: newest?.progress?.started_at_ms ?? partial?.created_at_ms ?? null,
      activity: runActivity(messages, live, runId),
    };
  }, [live, messages]);
}

function stop(connection: ReturnType<typeof useConnection>, runId: string) {
  connection
    .write("run.stop", { run_id: runId }, { retry: false })
    .then(() => toast("Stopped. The task waits for your message."))
    .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
}

/**
 * "Now": one row per task of the ticket that is at work or waits on someone, the main task first.
 * Each says who does what, how far the plan is, what the run did so far and how many questions
 * wait for you, with Stop and a way into the task's own conversation.
 */
export function NowCard({ tasks, rootId }: { tasks: readonly TaskView[]; rootId: string | null }) {
  const shown = nowTasks(tasks, rootId);
  if (shown.length === 0) return null;
  return (
    <section aria-label="Current status" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.card)}>
        {shown.map((t) => (
          <TaskNow key={t.id} task={t} main={t.id === rootId} />
        ))}
      </div>
    </section>
  );
}

function TaskNow({ task, main }: { task: TaskView; main: boolean }) {
  const navigate = useNavigate();
  const connection = useConnection();
  const { status } = useLinkState();
  const live = useTaskLive(task);
  const now = useNow(live.streaming);
  const step = planStep(live.todos);
  const counts = runCounts(live.activity);
  const state = taskStateLine(task);
  const sub = progressLine(step, live.streaming ? live.startedAtMs : null, now);
  const open = () => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } });
  return (
    <div data-now-task={task.id} {...stylex.props(styles.task)}>
      <div {...stylex.props(styles.taskHead)}>
        {task.employee_name ? (
          <Mark letter={initial(task.employee_name)} color={markColor(task.employee_id ?? "")} size={26} radius={7} />
        ) : null}
        <div {...stylex.props(styles.taskLines)}>
          <span {...stylex.props(styles.taskTitle)} title={task.title}>
            {nowTitle(task.employee_name, live.todos, task.title)}
          </span>
          <span {...stylex.props(styles.taskSub)}>
            {main ? "Main task" : "Delegated task"}
            {" · "}
            <span style={{ color: state.color }}>{state.text}</span>
            {sub ? ` · ${sub}` : ""}
          </span>
        </div>
        <div {...stylex.props(styles.tools)}>
          {live.streaming && live.runId ? (
            <Button
              size="sm"
              variant="ghost"
              label="Stop"
              isDisabled={status !== "online"}
              onClick={() => live.runId && stop(connection, live.runId)}
            />
          ) : null}
          <Button size="sm" variant="ghost" label={main ? "Open run" : "Open task"} onClick={open} />
        </div>
      </div>
      {step ? (
        <div
          role="progressbar"
          aria-label="Plan progress"
          aria-valuenow={step.done}
          aria-valuemin={0}
          aria-valuemax={step.total}
          {...stylex.props(styles.bar)}
        >
          {(live.todos ?? []).map((t, i) => (
            <span
              // biome-ignore lint/suspicious/noArrayIndexKey: a todo list has no ids; the order is what it is.
              key={i}
              {...stylex.props(
                styles.segment,
                t.state === "done" && styles.segmentDone,
                t.state === "doing" && styles.segmentDoing,
              )}
            />
          ))}
        </div>
      ) : null}
      <div {...stylex.props(styles.counts)}>
        <Count n={counts.read} one="file read" many="files read" />
        <span>
          <Count n={counts.changed} one="file changed" many="files changed" />
          {counts.changed > 0 && counts.added !== null ? (
            <>
              {" "}
              <span {...stylex.props(styles.plus)}>+{counts.added}</span>{" "}
              <span {...stylex.props(styles.minus)}>−{counts.removed}</span>
            </>
          ) : null}
        </span>
        <Count n={counts.commands} one="command run" many="commands run" />
        {task.pending_approvals > 0 ? (
          <button type="button" onClick={open} {...stylex.props(styles.ask)}>
            {questionsText(task.pending_approvals)}
          </button>
        ) : (
          <span>{questionsText(0)}</span>
        )}
      </div>
    </div>
  );
}

function Count({ n, one, many }: { n: number; one: string; many: string }) {
  return (
    <span>
      <span {...stylex.props(styles.num)}>{n}</span> {n === 1 ? one : many}
    </span>
  );
}

/** The Activity heading with the filter over the ticket's talk; folded updates fold in the list itself. */
export function ActivityHead({ filter, onFilter }: { filter: ActivityFilter; onFilter: (f: ActivityFilter) => void }) {
  return (
    <div {...stylex.props(styles.activityHead)}>
      <h2 {...stylex.props(styles.heading)}>Activity</h2>
      <span {...stylex.props(styles.grow)} />
      <fieldset aria-label="Filter activity" {...stylex.props(styles.filters)}>
        {ACTIVITY_FILTERS.map((f) => (
          <button
            key={f.key}
            type="button"
            aria-pressed={filter === f.key}
            onClick={() => onFilter(f.key)}
            {...stylex.props(styles.filter, filter === f.key && styles.filterOn)}
          >
            {f.label}
          </button>
        ))}
      </fieldset>
    </div>
  );
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

/** Above the composer: the call each busy task runs this second, with its command and a way to its output. */
export function LiveBar({ tasks, rootId }: { tasks: readonly TaskView[]; rootId: string | null }) {
  const shown = nowTasks(tasks, rootId).slice(0, LIVE_BAR_MAX);
  if (shown.length === 0) return null;
  return (
    <section aria-label="Running now" {...stylex.props(styles.live)}>
      {shown.map((t) => (
        <LiveLine key={t.id} task={t} />
      ))}
    </section>
  );
}

function LiveLine({ task }: { task: TaskView }) {
  const navigate = useNavigate();
  const live = useTaskLive(task);
  const now = useNow(live.streaming);
  const call = live.streaming ? currentCall(live.activity) : null;
  if (!live.streaming) return null;
  return (
    <div {...stylex.props(styles.liveRow)}>
      <span {...stylex.props(styles.pulse)} />
      {task.employee_name ? <span {...stylex.props(styles.liveWho)}>{task.employee_name}</span> : null}
      <span {...stylex.props(styles.liveWhat)}>{call ? call.label : "Thinking"}</span>
      {call?.detail ? <code {...stylex.props(styles.code)}>{call.detail}</code> : null}
      <span {...stylex.props(styles.grow)} />
      {live.startedAtMs !== null ? (
        <span {...stylex.props(styles.liveTime)}>{elapsed(now - live.startedAtMs)}</span>
      ) : null}
      <button
        type="button"
        onClick={() => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } })}
        {...stylex.props(styles.linkButton)}
      >
        Live output
      </button>
    </div>
  );
}

/**
 * The top of the details panel: the main task's plan, step by step, the step at work folding open
 * to its latest calls; below it every delegated task, each folding open to its own plan.
 */
export function PlanPanel({ tasks, root }: { tasks: readonly TaskView[]; root: TaskView | null }) {
  const delegated = useMemo(() => treeOrder(tasks.filter((t) => t.id !== root?.id)), [tasks, root?.id]);
  if (!root && delegated.length === 0) return null;
  return (
    <section aria-label="Plan" {...stylex.props(styles.plan)}>
      {root ? <MainPlan task={root} /> : null}
      {delegated.length > 0 ? (
        <section aria-label="Delegated tasks" {...stylex.props(styles.subtasks)}>
          <h3 {...stylex.props(styles.subHead)}>{`Tasks (${delegated.length})`}</h3>
          {delegated.map(({ task, depth }) => (
            <SubTask key={task.id} task={task} depth={depth} />
          ))}
        </section>
      ) : null}
    </section>
  );
}

function MainPlan({ task }: { task: TaskView }) {
  const live = useTaskLive(task);
  const step = planStep(live.todos);
  const [open, setOpen] = useState<Record<number, boolean>>({});
  const calls = lastSteps(live.activity, PLAN_STEPS_SHOWN);
  return (
    <>
      <div {...stylex.props(styles.planHead)}>
        <h2 {...stylex.props(styles.planTitle)}>Plan</h2>
        <span {...stylex.props(styles.planWho)}>
          {task.employee_name ? `Main task · ${task.employee_name}` : "Main task"}
        </span>
        {step ? <span {...stylex.props(styles.planCount)}>{`${step.done} / ${step.total}`}</span> : null}
      </div>
      {live.todos ? (
        live.todos.map((t, i) => {
          // Only the step at work has more to show: what the run does in it.
          const more = t.state === "doing" && live.streaming && calls.length > 0;
          const shown = more && (open[i] ?? true);
          const body = (
            <>
              <TodoMark state={t.state} />
              <span
                {...stylex.props(
                  styles.stepText,
                  t.state === "doing" ? styles.stepDoing : t.state === "done" ? styles.stepDone : styles.stepOpen,
                )}
              >
                {t.text}
              </span>
              {more ? (
                <span {...stylex.props(styles.chevron, !shown && styles.chevronShut)}>
                  <Icon name="chevronDown" size={14} />
                </span>
              ) : null}
            </>
          );
          return (
            // biome-ignore lint/suspicious/noArrayIndexKey: a todo list has no ids; the order is what it is.
            <div key={i}>
              {more ? (
                <button
                  type="button"
                  aria-expanded={shown}
                  onClick={() => setOpen({ ...open, [i]: !shown })}
                  {...stylex.props(styles.step, styles.stepButton)}
                >
                  {body}
                </button>
              ) : (
                <div {...stylex.props(styles.step)}>{body}</div>
              )}
              {shown ? (
                <ul aria-label="Latest steps" {...stylex.props(styles.calls)}>
                  {calls.map((c) => (
                    <li key={c.id} {...stylex.props(styles.call)}>
                      <span {...stylex.props(styles.callMark)}>
                        {c.state === "ok" ? (
                          <Icon name="check" size={11} />
                        ) : c.state === "failed" ? (
                          <StatusGlyph glyph="warn" color="var(--color-text-error)" />
                        ) : (
                          <StatusGlyph glyph="dot" color="var(--color-text-accent)" />
                        )}
                      </span>
                      <span {...stylex.props(styles.code)}>{c.text}</span>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          );
        })
      ) : (
        <p {...stylex.props(styles.empty)}>No plan yet. It shows here once the owner makes a todo list.</p>
      )}
    </>
  );
}

function SubTask({ task, depth }: { task: TaskView; depth: number }) {
  const [open, setOpen] = useState(false);
  const state = taskStateLine(task);
  return (
    <div {...stylex.props(styles.sub)} style={{ paddingLeft: 14 * depth }}>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} {...stylex.props(styles.subRow)}>
        <span {...stylex.props(styles.chevron, !open && styles.chevronShut)}>
          <Icon name="chevronDown" size={14} />
        </span>
        <span {...stylex.props(styles.subLines)}>
          <span {...stylex.props(styles.subTitle)} title={task.title}>
            {task.title}
          </span>
          <span {...stylex.props(styles.subWho)}>
            {task.employee_name ? `${task.employee_name} · ` : ""}
            <span style={{ color: state.color }}>{state.text}</span>
          </span>
        </span>
      </button>
      {open ? <SubTaskBody task={task} /> : null}
    </div>
  );
}

/** A delegated task opened in the Plan: its own todo list, and the way into its conversation. */
function SubTaskBody({ task }: { task: TaskView }) {
  const live = useTaskLive(task);
  return (
    <div {...stylex.props(styles.subBody)}>
      {live.todos ? (
        live.todos.map((t, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: a todo list has no ids; the order is what it is.
          <span key={i} {...stylex.props(styles.call)}>
            <TodoMark state={t.state} />
            <span
              {...stylex.props(
                t.state === "done" ? styles.stepDone : t.state === "doing" ? styles.stepDoing : styles.stepOpen,
              )}
            >
              {t.text}
            </span>
          </span>
        ))
      ) : (
        <p {...stylex.props(styles.empty)}>No plan yet.</p>
      )}
      <Link to="/tasks/$taskId" params={{ taskId: task.id }} {...stylex.props(styles.subLink)}>
        Open the task's conversation
      </Link>
    </div>
  );
}

/**
 * The work of every task of the ticket as Activity rows (ticket #76): read from each task's own
 * conversation, under the same keys as its run screen, with what streams in them now.
 */
export function useTicketWork(tasks: readonly TaskView[]): WorkRow[] {
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
    return workRows(
      tasks.flatMap((t) =>
        t.conversation_id
          ? [
              {
                taskId: t.id,
                who: t.employee_name ?? null,
                messages: byConversation.get(t.conversation_id) ?? [],
                live: live.filter((u) => u.conversation_id === t.conversation_id),
              },
            ]
          : [],
      ),
    );
  }, [stamp, ids, tasks, live]);
}

const zone = () => -new Date().getTimezoneOffset();

/** One row of a task's work in the ticket's Activity. */
export function WorkItem({ row }: { row: WorkRow }) {
  const title =
    row.kind === "finding" ? "Note" : row.kind === "running" ? runningTitle(row.step) : workTitle(row.work, row.steps);
  return (
    <article aria-label={row.who ? `${row.who} · ${title}` : title} {...stylex.props(styles.work)}>
      <span aria-hidden="true" {...stylex.props(styles.workMark, row.kind === "running" && styles.workMarkLive)}>
        {row.kind === "running" ? (
          <StatusGlyph glyph="dot" color="var(--color-text-accent)" />
        ) : (
          <Icon name={row.kind === "finding" ? "chat" : WORK_ICON[row.work]} size={14} />
        )}
      </span>
      <div {...stylex.props(styles.workBody)}>
        <div {...stylex.props(styles.workHead)}>
          <span {...stylex.props(styles.workTitle)}>
            {row.who ? <span {...stylex.props(styles.workWho)}>{row.who}</span> : null}
            {row.who ? " · " : ""}
            {title}
          </span>
          <span {...stylex.props(styles.grow)} />
          {Number.isFinite(row.atMs) ? (
            <time dateTime={new Date(row.atMs).toISOString()} {...stylex.props(styles.workTime)}>
              {clock(row.atMs, zone())}
            </time>
          ) : null}
        </div>
        {row.kind === "finding" ? (
          <Finding text={row.text} />
        ) : row.kind === "running" ? (
          <RunningCall row={row} />
        ) : row.work === "command" ? (
          row.steps.map((s) => <CommandResult key={s.call_id} step={s} />)
        ) : (
          <>
            <FileChips steps={row.steps} />
            {row.work === "edit" ? <EditDiff steps={row.steps} /> : null}
          </>
        )}
      </div>
    </article>
  );
}

const WORK_ICON = { read: "note", edit: "edit", command: "tasks" } as const;

function FileChips({ steps }: { steps: ActivityView[] }) {
  const files = rowFiles(steps);
  if (files.length === 0) return null;
  return (
    <ul aria-label="Files" {...stylex.props(styles.chips)} style={{ margin: 0, padding: 0, listStyle: "none" }}>
      {files.map((f) => (
        <li key={f} title={f} {...stylex.props(styles.chip)}>
          {fileName(f)}
        </li>
      ))}
    </ul>
  );
}

function EditDiff({ steps }: { steps: ActivityView[] }) {
  const [all, setAll] = useState(false);
  const lines = useMemo(() => rowDiff(steps), [steps]);
  const counts = diffCounts(lines);
  if (lines.every((l) => l.sign === "file")) return null;
  const shown = all ? lines : lines.slice(0, DIFF_LINES_SHOWN);
  return (
    <>
      <figure aria-label="Diff" {...stylex.props(styles.box)}>
        {shown.map((l, i) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: a diff's lines have no ids and never move.
            key={i}
            {...stylex.props(
              styles.diffLine,
              l.sign === "file" && styles.diffFile,
              l.sign === "+" && styles.diffAdd,
              l.sign === "-" && styles.diffDel,
            )}
          >
            {l.sign === "file" ? l.text : `${l.sign} ${l.text}`}
          </div>
        ))}
        {!all && lines.length > shown.length ? (
          <div {...stylex.props(styles.diffMore)}>{`… ${lines.length - shown.length} more lines`}</div>
        ) : null}
      </figure>
      <div {...stylex.props(styles.workActions)}>
        <span {...stylex.props(styles.counts)}>
          <span {...stylex.props(styles.plus)}>+{counts.added}</span>
          <span {...stylex.props(styles.minus)}>−{counts.removed}</span>
        </span>
        {lines.length > DIFF_LINES_SHOWN ? (
          <button type="button" aria-expanded={all} onClick={() => setAll(!all)} {...stylex.props(styles.linkButton)}>
            {all ? "Hide diff" : "View diff"}
          </button>
        ) : null}
      </div>
    </>
  );
}

function CommandResult({ step }: { step: ActivityView }) {
  const [all, setAll] = useState(false);
  const output = (step.output ?? "").trimEnd();
  const lines = output === "" ? [] : output.split("\n");
  const shown = all ? lines : lines.slice(0, RESULT_LINES_SHOWN);
  return (
    <div {...stylex.props(styles.box)}>
      <div {...stylex.props(styles.command)}>$ {stepLine(step)}</div>
      {lines.length > 0 ? (
        <pre {...stylex.props(styles.result, step.ok === false && styles.resultFailed)}>{shown.join("\n")}</pre>
      ) : null}
      {lines.length > shown.length ? (
        <button type="button" onClick={() => setAll(true)} {...stylex.props(styles.linkButton, styles.resultMore)}>
          {`Show ${lines.length - shown.length} more lines`}
        </button>
      ) : null}
    </div>
  );
}

function RunningCall({ row }: { row: Extract<WorkRow, { kind: "running" }> }) {
  const navigate = useNavigate();
  const files = workKind(row.step.tool) === "command" ? [] : rowFiles([row.step]);
  return (
    <>
      {files.length > 0 ? (
        <FileChips steps={[row.step]} />
      ) : (
        <div {...stylex.props(styles.box)}>
          <div {...stylex.props(styles.command)}>
            {workKind(row.step.tool) === "command" ? "$ " : ""}
            {stepLine(row.step)}
          </div>
        </div>
      )}
      <div {...stylex.props(styles.workActions)}>
        <button
          type="button"
          onClick={() => void navigate({ to: "/tasks/$taskId", params: { taskId: row.taskId } })}
          {...stylex.props(styles.linkButton)}
        >
          Live output
        </button>
      </div>
    </>
  );
}

function Finding({ text }: { text: string }) {
  const [all, setAll] = useState(false);
  const long = text.length > FINDING_CHARS_SHOWN;
  return (
    <>
      <blockquote {...stylex.props(styles.quote)}>
        {long && !all ? `${text.slice(0, FINDING_CHARS_SHOWN)}…` : text}
      </blockquote>
      {long ? (
        <div {...stylex.props(styles.workActions)}>
          <button type="button" onClick={() => setAll(!all)} {...stylex.props(styles.linkButton)}>
            {all ? "Show less" : "Show more"}
          </button>
        </div>
      ) : null}
    </>
  );
}
