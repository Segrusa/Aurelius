// The ticket's Activity tab (ticket #83, design/ticket-activity-plain.html): the ticket's plan at the
// top, then one card per delegated task in plain words: its state, a status line per plan step,
// what it does now and its steps. A question to a board member sits in the card of the employee
// who asks it. On a Done ticket the cards become "Who did what". The rules are in
// ticket-activity-plain-logic.ts; the reads are the ones the Timeline uses (useTicketWork).

import { arrived } from "@aurelius/client";
import type { ApprovalView, TaskView, TicketDetail, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useChannelFeed, useEmployees } from "../data";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { ApprovalCard, QUESTION_TOOL, questionsOf } from "./approval-card";
import { quickReplies, type Todo } from "./chat-logic";
import {
  type Ask,
  askOf,
  type CardState,
  cardGroups,
  cardState,
  firstSentence,
  minutes,
  nowSentence,
  optionLabel,
  type Segment,
  segments,
  stepText,
  stepWindow,
  taskTime,
} from "./ticket-activity-plain-logic";
import { Facts, pageStyles, type TaskLive, type TicketWork, useNow } from "./ticket-page";

const pulse = stylex.keyframes({ "50%": { opacity: 0.3 } });
const MOTION = "@media (prefers-reduced-motion: reduce)";

const styles = stylex.create({
  col: {
    display: "flex",
    flexDirection: "column",
    gap: 14,
    width: "100%",
    maxWidth: 980,
    boxSizing: "border-box",
    marginInline: "auto",
    paddingInline: { default: 24, "@media (max-width: 760px)": 12 },
    paddingTop: 16,
    paddingBottom: 28,
  },
  secHead: { display: "flex", alignItems: "baseline", gap: 8, marginTop: 4, marginBottom: -4 },
  secTitle: {
    margin: 0,
    fontSize: 12,
    fontWeight: 600,
    letterSpacing: "0.04em",
    textTransform: "uppercase",
    color: "var(--color-text-secondary)",
  },
  empty: {
    borderWidth: 1,
    borderStyle: "dashed",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: 10,
    padding: 14,
    textAlign: "center",
    color: "var(--precision-text3)",
  },
  // The Plan.
  plan: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    backgroundColor: "var(--color-background-card)",
  },
  planHead: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 10, paddingInline: 14, paddingTop: 12 },
  planTitle: { margin: 0, fontSize: 14, fontWeight: 600 },
  planWho: { fontSize: 12, color: "var(--precision-text3)" },
  planLine: { paddingInline: 14, paddingTop: 10 },
  planSteps: {
    margin: 0,
    marginTop: 10,
    padding: 0,
    paddingBottom: 6,
    listStyle: "none",
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  planStep: {
    display: "grid",
    gridTemplateColumns: "22px minmax(0, 1fr)",
    alignItems: "center",
    gap: 10,
    paddingInline: 14,
    paddingBlock: 7,
    borderTopWidth: { default: 1, ":first-child": 0 },
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  dotMark: {
    display: "grid",
    placeItems: "center",
    width: 18,
    height: 18,
    boxSizing: "border-box",
    borderRadius: "50%",
    borderWidth: 1.5,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    fontSize: 10,
    fontWeight: 800,
    color: "var(--precision-text3)",
  },
  dotDone: {
    borderColor: "transparent",
    color: "var(--color-text-success)",
    backgroundColor: "var(--color-success-muted)",
  },
  dotNow: { borderColor: "var(--color-text-accent)" },
  dotAsk: {
    borderColor: "transparent",
    color: "var(--color-text-warning)",
    backgroundColor: "var(--color-warning-muted)",
  },
  dotPulse: {
    width: 7,
    height: 7,
    borderRadius: "50%",
    backgroundColor: "var(--color-text-accent)",
    animationName: { default: pulse, [MOTION]: "none" },
    animationDuration: "1.4s",
    animationTimingFunction: "ease-in-out",
    animationIterationCount: "infinite",
  },
  stepTxt: { minWidth: 0 },
  stepName: { fontWeight: 500, color: "var(--color-text-primary)" },
  stepNameDone: { color: "var(--color-text-secondary)" },
  stepSub: { display: "block", fontSize: 12, color: "var(--precision-text3)" },
  making: { margin: 0, paddingInline: 14, paddingBlock: 12, color: "var(--precision-text3)" },
  // Status lines.
  segs: { display: "flex", gap: 3 },
  seg: { flexGrow: 1, flexBasis: 0, height: 5, borderRadius: 3, backgroundColor: "var(--color-border-emphasized)" },
  segDone: { backgroundColor: "var(--color-text-success)" },
  segNow: { backgroundColor: "var(--color-text-accent)", opacity: 0.5 },
  segAsk: { backgroundColor: "var(--color-text-warning)" },
  segMeta: {
    display: "flex",
    justifyContent: "space-between",
    marginTop: 4,
    fontSize: 11.5,
    color: "var(--precision-text3)",
  },
  // Cards.
  cards: {
    display: "grid",
    gridTemplateColumns: { default: "repeat(auto-fill, minmax(300px, 1fr))", "@media (max-width: 760px)": "1fr" },
    gap: 12,
  },
  card: {
    display: "flex",
    minWidth: 0,
    flexDirection: "column",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    backgroundColor: "var(--color-background-card)",
  },
  cardAsks: {
    gridColumn: "1 / -1",
    borderColor: "color-mix(in srgb, var(--color-text-warning) 55%, transparent)",
    boxShadow: "0 0 0 3px var(--color-warning-muted)",
  },
  cardLive: { borderColor: "color-mix(in srgb, var(--color-text-success) 30%, transparent)" },
  top: { display: "flex", alignItems: "center", gap: 10, paddingInline: 14, paddingTop: 12 },
  name: { fontWeight: 600, color: "var(--color-text-primary)" },
  role: { fontSize: 12, color: "var(--precision-text3)" },
  state: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    marginInlineStart: "auto",
    borderRadius: 999,
    paddingInline: 8,
    paddingBlock: 2,
    fontSize: 11.5,
    fontWeight: 600,
    whiteSpace: "nowrap",
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--color-neutral)",
  },
  stateWorking: { color: "var(--color-text-success)", backgroundColor: "var(--color-success-muted)" },
  stateAsking: { color: "var(--color-text-warning)", backgroundColor: "var(--color-warning-muted)" },
  stateStopped: { color: "var(--color-text-error)", backgroundColor: "var(--color-error-muted)" },
  dot: { width: 7, height: 7, borderRadius: "50%", backgroundColor: "currentColor" },
  dotLive: {
    animationName: { default: pulse, [MOTION]: "none" },
    animationDuration: "1.4s",
    animationTimingFunction: "ease-in-out",
    animationIterationCount: "infinite",
  },
  task: { paddingInline: 14, paddingTop: 8, fontSize: 12.5, color: "var(--color-text-secondary)" },
  taskName: { fontWeight: 600, color: "var(--color-text-primary)" },
  sline: { paddingInline: 14, paddingTop: 10 },
  now: { paddingInline: 14, paddingTop: 8, fontSize: 13.5, lineHeight: 1.45, color: "var(--color-text-primary)" },
  nowText: { display: "-webkit-box", WebkitBoxOrient: "vertical", WebkitLineClamp: 2, overflow: "hidden" },
  label: { display: "block", marginBottom: 1, fontSize: 11.5, color: "var(--precision-text3)" },
  steps: {
    display: "flex",
    flexDirection: "column",
    gap: 3,
    marginInline: 14,
    marginTop: 10,
    marginBottom: 0,
    paddingTop: 8,
    paddingInlineStart: 0,
    listStyle: "none",
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    fontSize: 12.5,
  },
  step: { display: "flex", alignItems: "baseline", gap: 8, color: "var(--color-text-secondary)" },
  stepNow: { fontWeight: 500, color: "var(--color-text-primary)" },
  stepIcon: { width: 14, flexShrink: 0, textAlign: "center", fontSize: 11 },
  iconDone: { color: "var(--color-text-success)" },
  iconNow: { color: "var(--color-text-accent)" },
  iconAsk: { color: "var(--color-text-warning)" },
  stepTime: {
    marginInlineStart: "auto",
    whiteSpace: "nowrap",
    fontSize: 11.5,
    fontWeight: 400,
    color: "var(--precision-text3)",
  },
  more: { fontSize: 12, color: "var(--precision-text3)", paddingInlineStart: 22 },
  facts: { paddingInline: 14, paddingTop: 8 },
  foot: {
    display: "flex",
    flexWrap: "wrap",
    gap: 4,
    marginTop: "auto",
    paddingInline: 8,
    paddingTop: 8,
    paddingBottom: 10,
  },
  // A card that asks: the work on the left, the question on the right.
  asksBody: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1fr) minmax(0, 1.2fr)",
      "@media (max-width: 760px)": "minmax(0, 1fr)",
    },
  },
  asksLeft: {
    paddingBottom: 4,
    borderRightWidth: { default: 1, "@media (max-width: 760px)": 0 },
    borderRightStyle: "solid",
    borderRightColor: "var(--precision-line2)",
  },
  q: { padding: 12, paddingInline: 14 },
  qLabel: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    fontSize: 11.5,
    fontWeight: 600,
    color: "var(--color-text-warning)",
  },
  qTitle: { marginBlock: 4, fontSize: 14, fontWeight: 600 },
  qText: { marginBottom: 8, fontSize: 12.5, color: "var(--color-text-secondary)", maxHeight: 220, overflowY: "auto" },
  opts: { display: "flex", flexDirection: "column", gap: 6 },
  opt: {
    display: "flex",
    flexDirection: "column",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border-emphasized)", ":hover": "var(--color-text-accent)" },
    borderRadius: 8,
    paddingInline: 10,
    paddingBlock: 7,
    cursor: "pointer",
    textAlign: "left",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-surface)",
  },
  optLabel: { fontSize: 12.5, fontWeight: 600 },
  optText: { fontSize: 12, color: "var(--precision-text3)" },
  pick: { marginInlineStart: 4, fontSize: 11, fontWeight: 600, color: "var(--color-text-accent)" },
  reply: { display: "flex", gap: 6, marginTop: 8 },
  input: {
    minWidth: 0,
    flexGrow: 1,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: 8,
    paddingInline: 10,
    paddingBlock: 6,
    font: "inherit",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-surface)",
  },
  send: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-accent)",
    borderRadius: 8,
    paddingInline: 12,
    paddingBlock: 5,
    cursor: "pointer",
    fontWeight: 500,
    color: "var(--color-on-dark)",
    backgroundColor: "var(--color-accent)",
  },
  answered: { fontSize: 12.5, color: "var(--color-text-success)" },
  planAsk: {
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  // A finished task while the ticket still runs: one row.
  doneRow: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    paddingInline: 12,
    paddingBlock: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 10,
    backgroundColor: "var(--color-background-surface)",
  },
  doneText: { minWidth: 0, flexGrow: 1, fontSize: 12.5, color: "var(--color-text-secondary)" },
  doneTime: { whiteSpace: "nowrap", fontSize: 12, color: "var(--precision-text3)" },
  doneRows: { display: "flex", flexDirection: "column", gap: 6 },
  hidden: {
    position: "absolute",
    width: 1,
    height: 1,
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
  // The ticket's tabs, under its title.
  tabs: { display: "flex", gap: 4, marginTop: 6 },
  tab: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    borderWidth: 0,
    borderBottomWidth: 2,
    borderBottomStyle: "solid",
    borderBottomColor: "transparent",
    paddingInline: 10,
    paddingTop: 7,
    paddingBottom: 9,
    cursor: "pointer",
    fontWeight: 500,
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    backgroundColor: "transparent",
  },
  tabOn: { borderBottomColor: "var(--color-text-accent)", color: "var(--color-text-primary)" },
  count: {
    minWidth: 18,
    boxSizing: "border-box",
    borderRadius: 999,
    paddingInline: 5,
    fontSize: 11,
    fontWeight: 700,
    textAlign: "center",
    color: "var(--color-text-warning)",
    backgroundColor: "var(--color-warning-muted)",
  },
});

/** What every task of the ticket asks a board member right now, by task id. */
export function useTicketAsks(detail: TicketDetail, t: TicketView, work: TicketWork): Map<string, Ask> {
  const { features } = useLinkState();
  const approvals = useApprovals();
  const conversation = features.includes("ticket_channels") ? (t.conversation_id ?? null) : null;
  const feed = useChannelFeed(conversation, conversation !== null);
  const discussion = feed.data?.messages;
  return useMemo(() => {
    const out = new Map<string, Ask>();
    if (t.column === "done") return out;
    for (const task of detail.tasks) {
      const ask = askOf(task, approvals.data ?? [], discussion ?? [], work.live(task.id).runIds);
      if (ask) out.set(task.id, ask);
    }
    return out;
  }, [detail.tasks, approvals.data, discussion, work, t.column]);
}

interface Card {
  task: TaskView;
  state: CardState;
  live: TaskLive;
  ask: Ask | null;
}

/**
 * The Activity tab: the decision of the moment, the Plan, the employees at work, and what is done.
 * `onTimeline` opens the Timeline with one employee's work.
 */
export function ActivityPanel({
  detail,
  t,
  work,
  asks,
  gate,
  description,
  onTimeline,
}: {
  detail: TicketDetail;
  t: TicketView;
  work: TicketWork;
  asks: Map<string, Ask>;
  gate: ReactNode;
  description: ReactNode;
  onTimeline: (who: string) => void;
}) {
  const rootId = detail.root_task?.id ?? t.root_task_id ?? null;
  const root = detail.tasks.find((x) => x.id === rootId) ?? detail.root_task ?? null;
  const done = t.column === "done";
  const cards: Card[] = detail.tasks
    .filter((x) => done || x.id !== rootId)
    .map((task) => {
      const ask = asks.get(task.id) ?? null;
      return { task, ask, live: work.live(task.id), state: cardState(task, detail.tasks, ask !== null) };
    });
  const groups = cardGroups(cards);
  const ticking = cards.some((c) => c.state.kind !== "done") || (root !== null && !done);
  const now = useNow(ticking, 30_000);
  const timeline = (who: string) => onTimeline(who);
  return (
    <div {...stylex.props(styles.col)}>
      {gate}
      {root ? (
        <PlanCard root={root} live={work.live(root.id)} ask={asks.get(root.id) ?? null} done={done} now={now} t={t} />
      ) : null}
      {done ? (
        cards.length > 0 ? (
          <>
            <SectionHead title="Who did what" />
            <div {...stylex.props(styles.cards)}>
              {cards.map((c) => (
                <SummaryCard key={c.task.id} card={c} now={now} onTimeline={timeline} />
              ))}
            </div>
          </>
        ) : null
      ) : (
        <>
          {groups.now.length > 0 ? (
            <>
              <SectionHead title="Working now" />
              <div {...stylex.props(styles.cards)}>
                {groups.now.map((c) => (
                  <EmployeeCard key={c.task.id} card={c} now={now} t={t} onTimeline={timeline} />
                ))}
              </div>
            </>
          ) : null}
          {groups.done.length > 0 ? (
            <>
              <SectionHead title="Done" />
              <div {...stylex.props(styles.doneRows)}>
                {groups.done.map((c) => (
                  <DoneRow key={c.task.id} card={c} now={now} onTimeline={timeline} />
                ))}
              </div>
            </>
          ) : null}
          {!root && cards.length === 0 ? (
            <div {...stylex.props(styles.empty)}>Nobody works on this ticket yet.</div>
          ) : null}
        </>
      )}
      {description}
    </div>
  );
}

function SectionHead({ title }: { title: string }) {
  return (
    <div {...stylex.props(styles.secHead)}>
      <h2 {...stylex.props(styles.secTitle)}>{title}</h2>
    </div>
  );
}

/** One segment per plan step; with no plan yet, one open segment. */
function StatusLine({ segs }: { segs: Segment[] }) {
  const shown: Segment[] = segs.length > 0 ? segs : ["open"];
  return (
    <div aria-hidden="true" {...stylex.props(styles.segs)}>
      {shown.map((s, i) => (
        <i
          // biome-ignore lint/suspicious/noArrayIndexKey: a plan's steps have no ids; the order is what it is.
          key={i}
          {...stylex.props(
            styles.seg,
            s === "done" && styles.segDone,
            s === "now" && styles.segNow,
            s === "ask" && styles.segAsk,
          )}
        />
      ))}
    </div>
  );
}

/** The ticket's plan: the main task's todo list, with what its current step does now. */
function PlanCard({
  root,
  live,
  ask,
  done,
  now,
  t,
}: {
  root: TaskView;
  live: TaskLive;
  ask: Ask | null;
  done: boolean;
  now: number;
  t: TicketView;
}) {
  const todos = live.todos;
  const segs = segments(todos, ask !== null);
  const who = root.employee_name ?? "The owner";
  const said = live.latest ? nowSentence(live.latest.text, live.latest.activity) : null;
  const meta = [
    "Main task",
    root.employee_name ?? null,
    todos ? stepText(todos).replace(/^Step/, "step") : null,
    taskTime(root, now),
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <section aria-label="Plan" {...stylex.props(styles.plan, ask && styles.cardAsks)}>
      <div {...stylex.props(styles.planHead)}>
        <h2 {...stylex.props(styles.planTitle)}>Plan</h2>
        <span {...stylex.props(styles.planWho)}>{meta}</span>
      </div>
      {todos ? (
        <>
          <div {...stylex.props(styles.planLine)}>
            <StatusLine segs={segs} />
          </div>
          <ol {...stylex.props(styles.planSteps)}>
            {todos.map((step, i) => (
              <PlanStep
                // biome-ignore lint/suspicious/noArrayIndexKey: a plan's steps have no ids; the order is what it is.
                key={i}
                step={step}
                n={i + 1}
                seg={segs[i] ?? "open"}
                sub={segs[i] === "ask" ? "Asked you a question" : segs[i] === "now" && !done ? said : null}
              />
            ))}
          </ol>
        </>
      ) : (
        <p {...stylex.props(styles.making)}>
          {done || root.status === "succeeded" ? "This ticket was done without a plan." : `${who} is making the plan…`}
        </p>
      )}
      {ask ? (
        <div {...stylex.props(styles.planAsk)}>
          <AskBox ask={ask} who={who} t={t} now={now} />
        </div>
      ) : null}
    </section>
  );
}

function PlanStep({ step, n, seg, sub }: { step: Todo; n: number; seg: Segment; sub: string | null }) {
  return (
    <li {...stylex.props(styles.planStep)}>
      <span
        aria-hidden="true"
        {...stylex.props(
          styles.dotMark,
          seg === "done" && styles.dotDone,
          seg === "now" && styles.dotNow,
          seg === "ask" && styles.dotAsk,
        )}
      >
        {seg === "done" ? "✓" : seg === "ask" ? "?" : seg === "now" ? <span {...stylex.props(styles.dotPulse)} /> : n}
      </span>
      <div {...stylex.props(styles.stepTxt)}>
        <span {...stylex.props(styles.hidden)}>
          {seg === "done" ? "Done: " : seg === "now" ? "Now: " : seg === "ask" ? "Asking: " : ""}
        </span>
        <b {...stylex.props(styles.stepName, seg === "done" && styles.stepNameDone)}>{step.text}</b>
        {sub ? <span {...stylex.props(styles.stepSub)}>{sub}</span> : null}
      </div>
    </li>
  );
}

function StateChip({ state, extra }: { state: CardState; extra?: string }) {
  return (
    <span
      {...stylex.props(
        styles.state,
        state.kind === "working" && styles.stateWorking,
        state.kind === "asking" && styles.stateAsking,
        state.kind === "stopped" && styles.stateStopped,
      )}
    >
      {state.kind === "working" || state.kind === "asking" ? (
        <span aria-hidden="true" {...stylex.props(styles.dot, state.kind === "working" && styles.dotLive)} />
      ) : null}
      {state.kind === "done" ? `✓ ${state.text}` : state.text}
      {extra ? ` · ${extra}` : ""}
    </span>
  );
}

function CardTop({ task, state, extra }: { task: TaskView; state: CardState; extra?: string }) {
  const employees = useEmployees();
  const e = employees.data?.find((x) => x.id === task.employee_id || x.name === task.employee_name);
  const name = task.employee_name ?? "An employee";
  return (
    <div {...stylex.props(styles.top)}>
      <Mark letter={initial(name)} color={markColor(task.employee_id ?? name)} size={28} radius={8} />
      <div>
        <div {...stylex.props(styles.name)}>{name}</div>
        {e ? <div {...stylex.props(styles.role)}>{e.title ?? e.role}</div> : null}
      </div>
      <StateChip state={state} {...(extra ? { extra } : {})} />
    </div>
  );
}

function CardFoot({
  task,
  onTimeline,
  open = true,
}: {
  task: TaskView;
  onTimeline: (who: string) => void;
  open?: boolean;
}) {
  const navigate = useNavigate();
  const name = task.employee_name;
  return (
    <div {...stylex.props(styles.foot)}>
      {name ? (
        <button type="button" onClick={() => onTimeline(name)} {...stylex.props(pageStyles.link)}>
          {`Timeline of ${name} →`}
        </button>
      ) : null}
      {open ? (
        <button
          type="button"
          onClick={() => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } })}
          {...stylex.props(pageStyles.link)}
        >
          Open task
        </button>
      ) : null}
    </div>
  );
}

/** What a card says it does now: its own words, the phrase of what runs, or why it stopped. */
function nowOf(card: Card): string {
  if (card.state.kind === "stopped") return firstSentence(card.task.result) ?? "It stopped before it was done.";
  const said = card.live.latest ? nowSentence(card.live.latest.text, card.live.latest.activity) : null;
  if (card.ask && card.state.kind === "asking") return said ?? "Asked you a question and waits for your answer.";
  return said ?? "Getting started…";
}

function CardWork({ card, now }: { card: Card; now: number }) {
  const { task, live, state } = card;
  const asking = state.kind === "asking";
  const segs = segments(live.todos, asking);
  const window = live.todos ? stepWindow(live.todos) : null;
  return (
    <>
      <div {...stylex.props(styles.task)}>
        Working on <b {...stylex.props(styles.taskName)}>{task.title}</b>
      </div>
      <div {...stylex.props(styles.sline)}>
        <StatusLine segs={segs} />
        <div {...stylex.props(styles.segMeta)}>
          <span>{stepText(live.todos)}</span>
          <span>{taskTime(task, now)}</span>
        </div>
      </div>
      <div {...stylex.props(styles.now)}>
        <span {...stylex.props(styles.label)}>Now</span>
        <span {...stylex.props(styles.nowText)}>{nowOf(card)}</span>
      </div>
      {window && window.shown.length > 0 ? (
        <ul aria-label="Steps" {...stylex.props(styles.steps)}>
          {window.earlier > 0 ? (
            <li
              {...stylex.props(styles.more)}
            >{`+${window.earlier} earlier ${window.earlier === 1 ? "step" : "steps"}`}</li>
          ) : null}
          {window.shown.map((s, i) => {
            const seg = segs[window.first + i] ?? "open";
            return (
              <li
                // biome-ignore lint/suspicious/noArrayIndexKey: a plan's steps have no ids; the order is what it is.
                key={i}
                {...stylex.props(styles.step, (seg === "now" || seg === "ask") && styles.stepNow)}
              >
                <span
                  aria-hidden="true"
                  {...stylex.props(
                    styles.stepIcon,
                    seg === "done" && styles.iconDone,
                    seg === "now" && styles.iconNow,
                    seg === "ask" && styles.iconAsk,
                  )}
                >
                  {seg === "done" ? "✓" : seg === "now" ? "●" : seg === "ask" ? "?" : "·"}
                </span>
                {s.text}
                {seg === "ask" ? <span {...stylex.props(styles.stepTime)}>waiting on you</span> : null}
                {seg === "now" && live.streaming ? <span {...stylex.props(styles.stepTime)}>now</span> : null}
              </li>
            );
          })}
          {window.later > 0 ? (
            <li {...stylex.props(styles.more)}>{`+${window.later} more ${window.later === 1 ? "step" : "steps"}`}</li>
          ) : null}
        </ul>
      ) : null}
    </>
  );
}

/** A delegated task at work, waiting, asking you or stopped. */
function EmployeeCard({
  card,
  now,
  t,
  onTimeline,
}: {
  card: Card;
  now: number;
  t: TicketView;
  onTimeline: (who: string) => void;
}) {
  const { task, state, ask } = card;
  const name = task.employee_name ?? "An employee";
  const asking = state.kind === "asking" && ask !== null;
  return (
    <article
      id={`ticket-card-${task.id}`}
      aria-label={`${name}, ${state.text.toLowerCase()}`}
      {...stylex.props(styles.card, asking && styles.cardAsks, state.kind === "working" && styles.cardLive)}
    >
      <CardTop task={task} state={state} />
      {asking ? (
        <div {...stylex.props(styles.asksBody)}>
          <div {...stylex.props(styles.asksLeft)}>
            <CardWork card={card} now={now} />
          </div>
          <AskBox ask={ask} who={name} t={t} now={now} />
        </div>
      ) : (
        <>
          <CardWork card={card} now={now} />
          <div {...stylex.props(styles.facts)}>
            <Facts steps={card.live.all} />
          </div>
          <CardFoot task={task} onTimeline={onTimeline} />
        </>
      )}
    </article>
  );
}

/** A finished task while the ticket still runs: one row with what it did. */
function DoneRow({ card, now, onTimeline }: { card: Card; now: number; onTimeline: (who: string) => void }) {
  const name = card.task.employee_name ?? "An employee";
  const said = firstSentence(card.task.result) ?? card.task.title;
  return (
    <div {...stylex.props(styles.doneRow)}>
      <Mark letter={initial(name)} color={markColor(card.task.employee_id ?? name)} size={20} radius={5} />
      <span {...stylex.props(styles.doneText)}>
        <b {...stylex.props(styles.taskName)}>{name}</b> · {said}
      </span>
      <span {...stylex.props(styles.doneTime)}>{taskTime(card.task, now)}</span>
      {card.task.employee_name ? (
        <button
          type="button"
          onClick={() => onTimeline(name)}
          aria-label={`Timeline of ${name}`}
          {...stylex.props(pageStyles.link)}
        >
          Timeline →
        </button>
      ) : null}
    </div>
  );
}

/** "Who did what" on a Done ticket: one sentence per employee, the whole status line and its facts. */
function SummaryCard({ card, now, onTimeline }: { card: Card; now: number; onTimeline: (who: string) => void }) {
  const { task, live, state } = card;
  return (
    <article
      aria-label={`${task.employee_name ?? "An employee"}, ${state.text.toLowerCase()}`}
      {...stylex.props(styles.card)}
    >
      <CardTop task={task} state={state} extra={taskTime(task, now)} />
      <div {...stylex.props(styles.sline)}>
        <StatusLine segs={segments(live.todos, false)} />
      </div>
      <div {...stylex.props(styles.now)}>
        <span {...stylex.props(styles.nowText)}>{firstSentence(task.result) ?? task.title}</span>
      </div>
      <div {...stylex.props(styles.facts)}>
        <Facts steps={live.all} />
      </div>
      <CardFoot task={task} onTimeline={onTimeline} open={false} />
    </article>
  );
}

const ago = (ms: number) => (ms < 60_000 ? "just now" : `${minutes(ms)} ago`);

/**
 * A question to the board member inside the asking employee's card. A question with options is
 * answered with one click or in your own words; a plan or an action keeps its own card.
 */
function AskBox({ ask, who, t, now }: { ask: Ask; who: string; t: TicketView; now: number }) {
  if (ask.kind === "approval") {
    const questions = ask.approval.tool === QUESTION_TOOL ? questionsOf(ask.approval) : [];
    const only = questions.length === 1 && !questions[0]?.multi ? questions[0] : null;
    if (!only)
      return (
        <div {...stylex.props(styles.q)}>
          <ApprovalCard approval={ask.approval} />
        </div>
      );
    return (
      <QuestionBox
        who={who}
        at={ask.approval.requested_at_ms}
        now={now}
        title={only.text}
        text={only.header && only.header !== only.text ? only.header : null}
        options={only.options}
        send={approvalAnswer(ask.approval, only.text)}
      />
    );
  }
  const replies = quickReplies(ask.message.activity);
  return (
    <QuestionBox
      who={who}
      at={ask.message.created_at_ms}
      now={now}
      title={replies?.question ?? null}
      markdown={ask.message.text}
      options={(replies?.options ?? []).map((label) => ({ label, description: "" }))}
      send={postAnswer(t)}
    />
  );
}

type Send = (connection: ReturnType<typeof useConnection>, text: string) => Promise<unknown>;

const approvalAnswer =
  (approval: ApprovalView, question: string): Send =>
  (connection, text) =>
    connection.write("approval.answer", { approval_id: approval.id, answers: { [question]: text } }, { retry: false });

const postAnswer =
  (t: TicketView): Send =>
  (connection, text) =>
    connection.write(
      "group_chat.send",
      { text, images: [], files: [], channel: t.conversation_id ?? null, thread_parent: null, confirm_secret: false },
      { retry: false },
    );

function QuestionBox({
  who,
  at,
  now,
  title,
  text = null,
  markdown = null,
  options,
  send,
}: {
  who: string;
  at: number;
  now: number;
  title: string | null;
  text?: string | null;
  markdown?: string | null;
  options: { label: string; description: string }[];
  send: Send;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const { status } = useLinkState();
  const [own, setOwn] = useState("");
  const [busy, setBusy] = useState(false);
  const [answered, setAnswered] = useState<string | null>(null);
  const answer = async (value: string) => {
    const clean = value.trim();
    if (clean === "" || busy) return;
    setBusy(true);
    try {
      await send(connection, clean);
      setAnswered(clean);
      void client.invalidateQueries({ queryKey: ["approvals"] });
    } catch (e) {
      if (arrived(e)) setAnswered(clean);
      else toast(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
    }
  };
  if (answered !== null)
    return (
      <div {...stylex.props(styles.q)}>
        <div role="status" {...stylex.props(styles.answered)}>
          ✓ You answered: <b>{answered}</b>. {who} goes on.
        </div>
      </div>
    );
  const offline = status !== "online";
  return (
    <div {...stylex.props(styles.q)}>
      <div {...stylex.props(styles.qLabel)}>{`? Question for you · ${ago(now - at)}`}</div>
      {title ? <h3 {...stylex.props(styles.qTitle)}>{title}</h3> : null}
      {text ? <p {...stylex.props(styles.qText)}>{text}</p> : null}
      {markdown ? (
        <div {...stylex.props(styles.qText)}>
          <Markdown text={markdown} />
        </div>
      ) : null}
      {options.length > 0 ? (
        <fieldset
          aria-label={`Answer ${who}`}
          {...stylex.props(styles.opts)}
          style={{ margin: 0, padding: 0, border: 0 }}
        >
          {options.map((o) => {
            const shown = optionLabel(o.label);
            return (
              <button
                key={o.label}
                type="button"
                disabled={busy || offline}
                onClick={() => void answer(shown.label)}
                {...stylex.props(styles.opt)}
              >
                <span {...stylex.props(styles.optLabel)}>
                  {shown.label}
                  {shown.pick ? <span {...stylex.props(styles.pick)}>{`${who}'s pick`}</span> : null}
                </span>
                {o.description ? <span {...stylex.props(styles.optText)}>{o.description}</span> : null}
              </button>
            );
          })}
        </fieldset>
      ) : null}
      <div {...stylex.props(styles.reply)}>
        <input
          aria-label={options.length > 0 ? "Or answer in your own words" : `Your answer to ${who}`}
          placeholder={options.length > 0 ? "Or answer in your own words…" : "Your answer…"}
          value={own}
          onChange={(e) => setOwn(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void answer(own);
          }}
          {...stylex.props(styles.input)}
        />
        <button
          type="button"
          disabled={busy || offline || own.trim() === ""}
          onClick={() => void answer(own)}
          {...stylex.props(styles.send)}
        >
          Send
        </button>
      </div>
    </div>
  );
}

/** A ticket's two views. */
export type TicketTab = "activity" | "timeline";

const TABS: { key: TicketTab; label: string }[] = [
  { key: "activity", label: "Activity" },
  { key: "timeline", label: "Timeline" },
];

/** The ticket's tabs, with the number of open questions for you on Activity; arrow keys move between them. */
export function TicketTabs({
  tab,
  onTab,
  questions,
}: {
  tab: TicketTab;
  onTab: (tab: TicketTab) => void;
  questions: number;
}) {
  const move = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const next = tab === "activity" ? "timeline" : "activity";
    onTab(next);
    e.currentTarget.querySelector<HTMLButtonElement>(`[data-tab="${next}"]`)?.focus();
  };
  return (
    <div role="tablist" aria-label="Ticket views" onKeyDown={move} {...stylex.props(styles.tabs)}>
      {TABS.map((x) => (
        <button
          key={x.key}
          type="button"
          role="tab"
          data-tab={x.key}
          id={`ticket-tab-${x.key}`}
          aria-controls={`ticket-panel-${x.key}`}
          aria-selected={tab === x.key}
          tabIndex={tab === x.key ? 0 : -1}
          onClick={() => onTab(x.key)}
          {...stylex.props(styles.tab, tab === x.key && styles.tabOn)}
        >
          {x.label}
          {x.key === "activity" && questions > 0 ? (
            <>
              <span aria-hidden="true" {...stylex.props(styles.count)}>
                {questions}
              </span>
              <span {...stylex.props(styles.hidden)}>
                {questions === 1 ? ", 1 question for you" : `, ${questions} questions for you`}
              </span>
            </>
          ) : null}
        </button>
      ))}
    </div>
  );
}
