// The overview a ticket's chat keeps beside the talk: a card above the composer with what runs now,
// the owner's todo list and the open tasks.
// The posts stay words only; this lives outside the feed.

import type { MessageView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useActivityOverview, useMessages, useTicket } from "../data";
import { usePlatform } from "../platform";
import { Icon, StatusGlyph } from "../ui/icons";
import { ticketActivityLine } from "./activity-bar-logic";
import { useLiveAll, useLiveStore } from "./chat-live";
import type { Todo } from "./chat-logic";
import { taskStateLine } from "./tasks-logic";
import { cardShown, latestTodos, nowLine, openTasks, taskTally, todoTally, workingLine } from "./ticket-overview-logic";

const styles = stylex.create({
  card: {
    marginInline: "auto",
    marginBottom: 8,
    display: "flex",
    width: "calc(100% - 32px)",
    maxWidth: 760,
    flexShrink: 0,
    flexDirection: "column",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 10,
    backgroundColor: "var(--color-background-surface)",
    fontSize: 13,
  },
  head: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    width: "100%",
    minHeight: 34,
    paddingInline: 12,
    borderWidth: 0,
    borderRadius: 10,
    cursor: "pointer",
    textAlign: "start",
    color: "var(--color-text-primary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  pulse: {
    width: 7,
    height: 7,
    flexShrink: 0,
    borderRadius: "50%",
    backgroundColor: "var(--precision-text3)",
  },
  pulseOn: { backgroundColor: "var(--color-text-accent)" },
  pulseWait: { backgroundColor: "var(--color-text-warning)" },
  headText: { minWidth: 0, flexGrow: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  quiet: { flexShrink: 0, fontSize: 12, color: "var(--color-text-secondary)" },
  turn: { display: "flex", flexShrink: 0, color: "var(--color-text-secondary)", transitionDuration: "120ms" },
  turned: { transform: "rotate(180deg)" },
  body: { display: "flex", flexDirection: "column", gap: 10, paddingInline: 12, paddingBottom: 10 },
  bodyScroll: { maxHeight: 220, overflowY: "auto" },
  list: { display: "flex", flexDirection: "column", gap: 4, margin: 0, padding: 0, listStyle: "none" },
  todo: { display: "flex", alignItems: "flex-start", gap: 8, lineHeight: "18px" },
  todoDone: { color: "var(--precision-text3)", textDecoration: "line-through" },
  todoDoing: { color: "var(--color-text-primary)", fontWeight: 500 },
  todoOpen: { color: "var(--color-text-secondary)" },
  mark: { display: "flex", flexShrink: 0, height: 18, alignItems: "center" },
  label: {
    margin: 0,
    fontSize: 11,
    fontWeight: 600,
    letterSpacing: "0.04em",
    textTransform: "uppercase",
    color: "var(--precision-text3)",
  },
  task: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    minHeight: 24,
    borderRadius: 6,
    paddingInline: 4,
    textDecoration: "none",
    color: "var(--color-text-primary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  taskTitle: { minWidth: 0, flexGrow: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  taskWho: { flexShrink: 0, fontSize: 12, color: "var(--color-text-secondary)" },
  taskState: { flexShrink: 0, fontSize: 12 },
});

/** Every second while something runs, so the timer moves. */
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

export interface TicketProgress {
  ticketId: string | null;
  /** Something runs on the ticket now (the owner, a delegate, or an answer in its chat). */
  running: boolean;
  /** Waiting, paused or running, in words, from the activity overview; null when nothing goes on. */
  stateLine: string | null;
  stateKind: "running" | "waiting" | "idle";
  /** "Running a command · 1m 36s" while an answer streams. */
  now: string | null;
  todos: Todo[] | null;
  open: { task: TaskView; depth: number }[];
  tally: string | null;
  tasks: TaskView[];
  rootId: string | null;
}

/** What the card and the panel show; `chatLive`: what streams in the ticket's own chat. */
export function useTicketProgress(
  ticketId: string | null,
  chatLive: { activity: MessageView["activity"]; progress: { started_at_ms: number } | null }[],
): TicketProgress {
  const detail = useTicket(ticketId);
  const overview = useActivityOverview();
  const root = detail.data?.root_task ?? null;
  const rootConversation = root?.conversation_id ?? null;
  const rootMessages = useMessages(rootConversation);
  const store = useLiveStore(rootConversation);
  const rootLive = useLiveAll(store);
  const messages: MessageView[] = useMemo(() => rootMessages.data?.messages ?? [], [rootMessages.data]);
  useEffect(() => {
    store.settle(new Set(messages.filter((m) => m.status !== "partial").map((m) => m.id)));
  }, [messages, store]);

  const streaming = [...rootLive, ...chatLive];
  const newest = streaming.at(-1) ?? null;
  const activity = (ticketId && overview.data?.tickets.find((a) => a.ticket_id === ticketId)) || null;
  const tasks = detail.data?.tasks ?? [];
  const working = workingLine(tasks);
  const running = streaming.length > 0 || activity?.state === "running" || working !== null;
  const stateKind = running ? "running" : activity ? "waiting" : "idle";
  const nowMs = useNow(running);
  const rootId = root?.id ?? detail.data?.ticket.root_task_id ?? null;
  const since = newest?.progress?.started_at_ms ?? (activity?.state === "running" ? activity.since_ms : null);
  return {
    ticketId,
    running,
    stateLine: activity ? ticketActivityLine(activity) : working,
    stateKind,
    now: newest ? nowLine(newest.activity, since, nowMs) : running && since !== null ? nowLine([], since, nowMs) : null,
    todos: latestTodos(rootLive, messages),
    open: openTasks(tasks, rootId),
    tally: taskTally(tasks, rootId),
    tasks,
    rootId,
  };
}

export function TodoMark({ state }: { state: Todo["state"] }) {
  return (
    <span {...stylex.props(styles.mark)}>
      {state === "done" ? (
        <span style={{ color: "var(--color-text-success, var(--color-text-accent))", display: "flex" }}>
          <Icon name="check" size={14} />
        </span>
      ) : state === "doing" ? (
        <StatusGlyph glyph="half" color="var(--color-text-accent)" />
      ) : (
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="8" cy="8" r="6.5" fill="none" stroke="var(--precision-text3)" strokeWidth="1.5" />
        </svg>
      )}
    </span>
  );
}

export function TodoList({ todos }: { todos: Todo[] }) {
  return (
    <ul aria-label="Todo list" {...stylex.props(styles.list)}>
      {todos.map((t, i) => (
        // A todo list has no ids; the order is what it is.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above.
        <li key={i} data-todo={t.state} {...stylex.props(styles.todo)}>
          <TodoMark state={t.state} />
          <span
            {...stylex.props(
              t.state === "done" ? styles.todoDone : t.state === "doing" ? styles.todoDoing : styles.todoOpen,
            )}
          >
            {t.text}
          </span>
        </li>
      ))}
    </ul>
  );
}

export function TaskLinks({ rows }: { rows: { task: TaskView; depth: number }[] }) {
  return (
    <ul aria-label="Tasks" {...stylex.props(styles.list)}>
      {rows.map(({ task, depth }) => {
        const state = taskStateLine(task);
        return (
          <li key={task.id}>
            <Link
              to="/tasks/$taskId"
              params={{ taskId: task.id }}
              {...stylex.props(styles.task)}
              style={{ paddingLeft: 4 + 14 * depth }}
            >
              <span {...stylex.props(styles.taskTitle)}>{task.title}</span>
              {task.employee_name ? <span {...stylex.props(styles.taskWho)}>{task.employee_name}</span> : null}
              <span {...stylex.props(styles.taskState)} style={{ color: state.color }}>
                {state.text}
              </span>
            </Link>
          </li>
        );
      })}
    </ul>
  );
}

const CARD_KEY = "chat.ticket-progress.open";

/** Above the composer of a ticket's chat: what runs now, the owner's todo list, the open tasks. */
export function ProgressCard({ p }: { p: TicketProgress }) {
  const { storage } = usePlatform();
  const [open, setOpen] = useState(() => storage.get(CARD_KEY) !== "0");
  const toggle = () => {
    storage.set(CARD_KEY, open ? "0" : "1");
    setOpen(!open);
  };
  if (!cardShown(p.running || p.stateKind === "waiting", p.todos, p.open.length)) return null;
  const head = p.now ?? p.stateLine ?? (p.todos ? "Todo list" : "Open tasks");
  const doing = p.todos?.find((t) => t.state === "doing")?.text;
  return (
    <section aria-label="Progress" data-ticket-progress {...stylex.props(styles.card)}>
      <button type="button" aria-expanded={open} onClick={toggle} {...stylex.props(styles.head)}>
        <span
          {...stylex.props(
            styles.pulse,
            p.stateKind === "running" && styles.pulseOn,
            p.stateKind === "waiting" && styles.pulseWait,
          )}
        />
        <span {...stylex.props(styles.headText)}>{!open && doing ? `${head} · ${doing}` : head}</span>
        {p.todos ? <span {...stylex.props(styles.quiet)}>{todoTally(p.todos)}</span> : null}
        <span {...stylex.props(styles.turn, open && styles.turned)}>
          <Icon name="chevronDown" size={14} />
        </span>
      </button>
      {open ? (
        <div {...stylex.props(styles.body, styles.bodyScroll)}>
          {p.todos ? <TodoList todos={p.todos} /> : null}
          {p.open.length > 0 ? (
            <>
              <p {...stylex.props(styles.label)}>{`Open tasks${p.tally ? ` · ${p.tally}` : ""}`}</p>
              <TaskLinks rows={p.open} />
            </>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
