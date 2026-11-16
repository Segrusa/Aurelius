import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ChatToolCalls } from "@astryxdesign/core/Chat";
import { Heading } from "@astryxdesign/core/Heading";
import { LayoutHeader } from "@astryxdesign/core/Layout";
import { Link } from "@astryxdesign/core/Link";
import type { StreamUpdate } from "@aurelius/client";
import type { ActivityView, MessageView, TaskRun, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useApprovals, useEmployees, useMessages, useProjects } from "../data";
import { hidden } from "../hidden";
import { useAction, useKeyContext } from "../keys/provider";
import { AttachedImages, useImagePaste } from "../ui/attach";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { ApprovalCard } from "./approval-card";
import { ActivityOutput, ContextDialog } from "./chat-message";
import { Held, useModels, useSend } from "./dialogs";
import { nextMode } from "./employee-logic";
import { Confirm, Page, Problem } from "./org-kit";
import { boardOnlyReason, lastRunOf, usageTotals } from "./run-logic";
import {
  FilesWritten,
  RunAside,
  RunGuidelines,
  SideProperties,
  SideRow,
  SideSection,
  UsageTotalsLine,
} from "./run-side";
import {
  FileChips,
  KeptFiles,
  LoadBanner,
  MarkdownChips,
  RunMenu,
  SubagentsAtWork,
  TraceLink,
  UsageChip,
  useRetryAnswer,
} from "./run-tools";
import { modelName } from "./settings-kit";
import { filesWritten, isActive, stepLine, stepsOf, subtasksOf, taskStateLine } from "./tasks-logic";

const styles = stylex.create({
  steps: { display: "flex", flexDirection: "column" },
  stepsHead: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  stepsCount: { fontWeight: 500, color: "var(--color-text-primary)" },
  failed: { fontSize: 12, color: "var(--color-text-error)" },
  system: { textAlign: "center", fontSize: 12, color: "var(--precision-text3)" },
  message: { display: "flex", gap: 12 },
  bubble: {
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 12,
    paddingBlock: 10,
  },
  who: { display: "flex", alignItems: "baseline", gap: 8 },
  name: { fontWeight: 500 },
  time: { fontSize: 12, color: "var(--precision-text3)" },
  working: { fontSize: 12, color: "var(--color-text-accent)" },
  bubbleText: { marginTop: 6 },
  answer: {
    display: "flex",
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    flexDirection: "column",
    gap: 10,
  },
  streamText: { margin: 0, fontSize: 14, lineHeight: 1.65, whiteSpace: "pre-wrap" },
  composer: {
    marginInline: "auto",
    display: "flex",
    width: "100%",
    maxWidth: 760,
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 8,
    paddingInline: { default: 40, "@media (max-width: 900px)": 16 },
    paddingBottom: 18,
  },
  box: {
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--precision-sunk)",
  },
  // A plain textarea: the specs find it by its name, and it never needs a field's chrome.
  textarea: {
    display: "block",
    width: "100%",
    boxSizing: "border-box",
    resize: "none",
    borderWidth: 0,
    backgroundColor: "transparent",
    paddingInline: 14,
    paddingBlock: 12,
    fontSize: 14,
    color: "var(--color-text-primary)",
    outline: "none",
    "::placeholder": { color: "var(--precision-text3)" },
  },
  composerBar: { display: "flex", alignItems: "center", gap: 6, paddingInline: 8, paddingBottom: 8 },
  queued: { marginInlineStart: "auto", fontSize: 12, color: "var(--precision-text3)" },
  head: {
    display: "flex",
    minHeight: 44,
    alignItems: "center",
    gap: 8,
    paddingInlineStart: 18,
    paddingInlineEnd: 12,
    color: "var(--precision-text3)",
  },
  truncate: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  chevron: { color: "var(--color-text-disabled)" },
  ticketLink: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  title: {
    margin: 0,
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontSize: 13,
    fontWeight: 400,
    color: "var(--color-text-primary)",
  },
  state: {
    marginInlineStart: 8,
    display: "inline-flex",
    height: 22,
    flexShrink: 0,
    alignItems: "center",
    gap: 6,
    borderRadius: 11,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 9,
    fontSize: 12,
    fontWeight: 500,
  },
  actions: { marginInlineStart: "auto", display: "flex", flexShrink: 0, alignItems: "center", gap: 6 },
  body: { display: "flex", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  run: { display: "flex", minWidth: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", flexDirection: "column" },
  scroll: { minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", overflowY: "auto" },
  thread: {
    marginInline: "auto",
    display: "flex",
    width: "100%",
    maxWidth: 760,
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 20,
    paddingInline: { default: 40, "@media (max-width: 900px)": 16 },
    paddingTop: 28,
    paddingBottom: 16,
  },
  taskTitle: { margin: 0, fontSize: 22, fontWeight: 600, letterSpacing: "-0.015em" },
  goal: {
    marginTop: 6,
    marginBottom: 0,
    fontSize: 14,
    lineHeight: 1.6,
    whiteSpace: "pre-wrap",
    color: "var(--color-text-secondary)",
  },
  subtask: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    paddingBlock: 4,
    color: { default: "var(--color-text-primary)", ":hover": "var(--color-text-accent)" },
  },
  subtaskTitle: {
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  subtaskState: { flexShrink: 0, fontSize: 12 },
  loading: { margin: 0, padding: 18, color: "var(--precision-text3)" },
});

const clock = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

export function useTaskCancel() {
  const connection = useConnection();
  const client = useQueryClient();
  return async (task: TaskView) => {
    try {
      await connection.write("task.cancel", { task_id: task.id }, { retry: false });
      await client.invalidateQueries({ queryKey: ["tasks"] });
      toast("The task is cancelled.");
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
}

/** The count and what failed are said here: ChatToolCalls only shows the latest call while it is folded. */
function Steps({ activity, messageId }: { activity: ActivityView[]; messageId: string }) {
  const steps = stepsOf(activity);
  if (!steps) return null;
  return (
    <div {...stylex.props(styles.steps)}>
      <div {...stylex.props(styles.stepsHead)}>
        <span {...stylex.props(styles.stepsCount)}>
          {steps.count} {steps.count === 1 ? "step" : "steps"}
        </span>
        {steps.failed > 0 ? <span {...stylex.props(styles.failed)}>{steps.failed} failed</span> : null}
      </div>
      <ChatToolCalls
        label="Steps"
        calls={activity.map((step) => ({
          name: step.tool,
          status: step.ok == null ? ("running" as const) : step.ok ? ("complete" as const) : ("error" as const),
          target: stepLine(step),
          key: `${step.call_id}${step.offset}`,
          ...(step.ok === false && step.output ? { errorMessage: step.output } : {}),
          resultDetail: <ActivityOutput step={step} messageId={messageId} />,
        }))}
      />
    </div>
  );
}

const Message = memo(function Message({
  m,
  name,
  employeeId,
  conversationId,
}: {
  m: MessageView;
  name: string;
  employeeId: string;
  conversationId?: string | null | undefined;
}) {
  if (m.deleted_at_ms) return null;
  if (m.system) return <div {...stylex.props(styles.system)}>{m.text}</div>;
  // Not the employee's words: the user's, or the assignment the server wrote for them.
  if (m.role === "user" || m.update === "brief") {
    const who = m.author_user?.username ?? (m.role === "user" ? "You" : "Assignment");
    return (
      <div {...stylex.props(styles.message)}>
        <Mark letter={initial(who)} color="#55595F" size={22} radius={11} />
        <div {...stylex.props(styles.bubble)}>
          <div {...stylex.props(styles.who)}>
            <span {...stylex.props(styles.name)}>{who}</span>
            <span {...stylex.props(styles.time)}>{m.queued ? "Queued" : clock(m.created_at_ms)}</span>
          </div>
          <div {...stylex.props(styles.bubbleText)}>
            <Markdown text={m.text} />
          </div>
        </div>
      </div>
    );
  }
  return (
    <div {...stylex.props(styles.message)}>
      <Mark letter={initial(name)} color={markColor(employeeId)} size={22} radius={6} />
      <div {...stylex.props(styles.answer)}>
        <div {...stylex.props(styles.who)}>
          <span {...stylex.props(styles.name)}>{name}</span>
          <span {...stylex.props(styles.time)}>{clock(m.created_at_ms)}</span>
          {m.status === "failed" ? (
            <span {...stylex.props(styles.failed)}>Failed</span>
          ) : m.status === "interrupted" ? (
            <span {...stylex.props(styles.time)}>Stopped</span>
          ) : null}
        </div>
        <Steps activity={m.activity} messageId={m.id} />
        {m.text ? <Markdown text={m.text} /> : null}
        <MarkdownChips
          text={m.text}
          scope={conversationId ? { kind: "conversation", conversation_id: conversationId } : null}
        />
        <FileChips files={m.uploads} />
        <KeptFiles files={m.files ?? []} />
        {m.run_id && m.status !== "partial" ? <TraceLink run={m.run_id} /> : null}
      </div>
    </div>
  );
});

/** The latest stream update of a conversation, or of one message in it when `messageId` is given. */
function useStreamUpdate(conversationId: string, messageId?: string): StreamUpdate | null {
  const connection = useConnection();
  const [update, setUpdate] = useState<StreamUpdate | null>(null);
  useEffect(
    () =>
      connection.streams.on(
        (u) =>
          u.conversation_id === conversationId &&
          (messageId === undefined || u.message_id === messageId) &&
          setUpdate(u),
      ),
    [connection, conversationId, messageId],
  );
  return update;
}

/** A new answer the list does not have yet: only this re-renders while it streams. */
function Streaming({
  conversationId,
  name,
  employeeId,
  known,
}: {
  conversationId: string;
  name: string;
  employeeId: string;
  known: Set<string>;
}) {
  const update = useStreamUpdate(conversationId);
  if (!update || known.has(update.message_id)) return null;
  return <LiveAnswer update={update} name={name} employeeId={employeeId} />;
}

/**
 * An answer the server saved halfway: it follows its own stream, so the saved half and the
 * live answer are never shown both, one below the other.
 */
function PartialMessage({
  m,
  name,
  employeeId,
  conversationId,
}: {
  m: MessageView;
  name: string;
  employeeId: string;
  conversationId: string;
}) {
  const update = useStreamUpdate(conversationId, m.id);
  if (!update) return <Message m={m} name={name} employeeId={employeeId} conversationId={conversationId} />;
  return <LiveAnswer update={update} name={name} employeeId={employeeId} />;
}

function LiveAnswer({ update, name, employeeId }: { update: StreamUpdate; name: string; employeeId: string }) {
  return (
    <div {...stylex.props(styles.message)}>
      <Mark letter={initial(name)} color={markColor(employeeId)} size={22} radius={6} />
      <div {...stylex.props(styles.answer)}>
        <div {...stylex.props(styles.who)}>
          <span {...stylex.props(styles.name)}>{name}</span>
          <span {...stylex.props(styles.working)}>Working…</span>
        </div>
        <Steps activity={update.activity} messageId={update.message_id} />
        {update.text ? <p {...stylex.props(styles.streamText)}>{update.text}</p> : null}
      </div>
    </div>
  );
}

function Composer({
  task,
  name,
  live,
  activeRun,
  run,
  waiting,
}: {
  task: TaskView;
  activeRun: string | null;
  name: string;
  live: boolean;
  run: TaskRun | undefined;
  waiting: boolean;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const [text, setText] = useState("");
  const area = useRef<HTMLTextAreaElement>(null);
  const models = useModels()
    .map((m) => m.key)
    .filter((m) => m !== "default");
  useAction("FocusComposer", () => area.current?.focus());
  const images = useImagePaste();
  const employees = useEmployees();
  const paused = employees.data?.find((e) => e.id === task.employee_id)?.status === "paused";
  const { busy, held, error, send, sendAnyway } = useSend(() => {
    setText("");
    void client.invalidateQueries({ queryKey: ["messages", task.conversation_id] });
  });
  const go = () => {
    if (paused) return;
    const conversationId = task.conversation_id;
    if (!conversationId || text.trim() === "" || busy) return;
    send(async (confirmSecret) => {
      const message = {
        conversation_id: conversationId,
        text: text.trim(),
        confirm_secret: confirmSecret,
        ...(images.paths.length > 0 ? { images: images.paths } : {}),
      };
      // The server keeps an employee's conversations apart from plain chats.
      await (task.employee_id
        ? connection.write("employee.send_message", { ...message, employee_id: task.employee_id })
        : connection.write("chat.send", message));
      images.clear();
      return "";
    });
  };
  // Stops what runs now, then this message goes: the same as in a channel.
  const sendNow = async () => {
    if (!activeRun || text.trim() === "") return;
    try {
      await connection.write("run.stop", { run_id: activeRun }, { retry: false });
    } catch {
      // It may have just ended: the message goes either way.
    }
    go();
  };
  useAction("SendNow", () => void sendNow(), Boolean(activeRun));
  if (!task.conversation_id) return null;
  return (
    <div {...stylex.props(styles.composer)}>
      <LoadBanner waiting={waiting} />
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem>{error}</Problem> : null}
      {paused ? (
        <Banner
          status="warning"
          role="status"
          title={`${name} is paused: nothing starts until you resume them on their page.`}
        />
      ) : null}
      <div {...images.target} {...stylex.props(styles.box)}>
        <AttachedImages images={images} />
        <textarea
          ref={area}
          aria-label={`Message ${name}`}
          rows={2}
          placeholder={`Message ${name}`}
          {...stylex.props(styles.textarea)}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
              e.preventDefault();
              go();
            }
          }}
        />
        <div {...stylex.props(styles.composerBar)}>
          {run ? <RunMenu conversationId={task.conversation_id} run={run} models={models} /> : null}
          <span {...stylex.props(styles.queued)}>{live ? "Queued until the step finishes" : ""}</span>
          <Button variant="primary" label="Send" isDisabled={busy || paused || text.trim() === ""} onClick={go} />
        </div>
      </div>
    </div>
  );
}

export function TaskRunScreen({ taskId }: { taskId: string }) {
  useKeyContext("TaskRun");
  const navigate = useNavigate();
  const connection = useConnection();
  const { status } = useLinkState();
  const tasks = useAllTasks();
  const projects = useProjects();
  const approvals = useApprovals();
  const cancel = useTaskCancel();
  const task = tasks.data?.find((t) => t.id === taskId);
  const messages = useMessages(task?.conversation_id);
  const list = useMemo(() => messages.data?.messages ?? [], [messages.data]);
  const known = useMemo(() => new Set(list.map((m) => m.id)), [list]);
  const lastRun = lastRunOf(list);
  const files = useMemo(() => filesWritten(list), [list]);
  const usage = useMemo(() => usageTotals(list), [list]);
  const subtasks = useMemo(() => subtasksOf(tasks.data ?? [], taskId), [tasks.data, taskId]);
  const end = useRef<HTMLDivElement>(null);
  const count = list.length;
  useEffect(() => {
    if (count > 0) end.current?.scrollIntoView({ block: "end" });
  }, [count]);
  useAction("Back", () => void navigate({ to: "/tasks" }));
  const retry = useRetryAnswer(task?.conversation_id, list, { employeeId: task?.employee_id ?? null });
  const [contextOf, setContextOf] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  useAction("ShowContext", () => {
    if (lastRun) setContextOf(lastRun);
    else toast("Open a conversation with an answer to see the context it used.");
  });
  const client = useQueryClient();
  useAction("CycleAgentMode", () => {
    const id = task?.conversation_id;
    const mode = messages.data?.agent_mode;
    if (!id || !mode) return;
    connection
      .write("conversation.set_agent_mode", { conversation_id: id, mode: nextMode(mode) }, { retry: false })
      .then(() => client.invalidateQueries({ queryKey: ["messages", id] }))
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  });
  useAction("RetryAnswer", retry);
  const stop = async () => {
    if (!lastRun) return;
    try {
      await connection.write("run.stop", { run_id: lastRun }, { retry: false });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  useAction("StopGeneration", () => void stop(), Boolean(task?.live_run));

  if (!task) {
    return (
      <Page>
        <LayoutHeader hasDivider padding={0} data-titlebar>
          <div {...stylex.props(styles.head)}>
            <Heading level={1} xstyle={styles.title}>
              Task
            </Heading>
          </div>
        </LayoutHeader>
        <p {...stylex.props(styles.loading)}>
          {tasks.error?.message ?? (tasks.isPending ? "Loading the task…" : "This task is not on this server.")}
        </p>
      </Page>
    );
  }

  const state = taskStateLine(task);
  const name = task.employee_name ?? "The employee";
  const employeeId = task.employee_id ?? task.id;
  const live = Boolean(task.live_run);
  const pending = (approvals.data ?? []).filter(
    (a) =>
      a.status === "pending" &&
      (a.task_id === task.id || (task.conversation_id && a.conversation_id === task.conversation_id)),
  );
  const project = projects.data?.find((p) => p.id === task.project_id);
  const lastAnswer = [...list].reverse().find((m) => m.role === "assistant");

  return (
    <Page>
      <LayoutHeader hasDivider padding={0} data-titlebar>
        <div {...stylex.props(styles.head)}>
          {project ? (
            <>
              <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={4} />
              <span {...stylex.props(styles.truncate)}>{project.name}</span>
              <span {...stylex.props(styles.chevron)}>›</span>
            </>
          ) : null}
          {task.ticket_id ? (
            <>
              <Link href={`/board?ticket=${task.ticket_id}`} color="inherit" xstyle={styles.ticketLink}>
                #{task.ticket_number}
              </Link>
              <span {...stylex.props(styles.chevron)}>›</span>
            </>
          ) : null}
          <Heading level={1} xstyle={styles.title}>
            Run by {name}
          </Heading>
          <span {...stylex.props(styles.state)} style={{ color: state.color }}>
            {state.text}
          </span>
          <div {...stylex.props(styles.actions)}>
            <UsageChip taskId={task.id} />
            <Menu
              label="Conversation"
              options={[
                { key: "context", label: "Show context of last answer" },
                ...(task.conversation_id ? [{ key: "delete", label: "Delete conversation…", danger: true }] : []),
              ]}
              onPick={(key) => {
                if (key === "delete") setDeleting(true);
                else if (lastRun) setContextOf(lastRun);
                else toast("Open a conversation with an answer to see the context it used.");
              }}
              trigger={<Button label="Conversation">⋯</Button>}
            />
            {!live && lastAnswer && (lastAnswer.status === "failed" || lastAnswer.status === "interrupted") ? (
              <Button label="Retry answer" onClick={retry} />
            ) : null}
            {live && lastRun ? (
              <Button label="Stop run" onClick={() => void stop()} />
            ) : isActive(task) && boardOnlyReason(task) === null ? (
              <Button variant="destructive" label="Cancel task" onClick={() => void cancel(task)} />
            ) : null}
          </div>
        </div>
      </LayoutHeader>
      {contextOf ? <ContextDialog run={contextOf} onClose={() => setContextOf(null)} /> : null}
      {deleting && task.conversation_id ? (
        <Confirm
          danger
          title="Delete this conversation?"
          body="Its messages are gone for good. The task itself stays."
          action="Delete"
          onClose={() => setDeleting(false)}
          onConfirm={() => {
            setDeleting(false);
            connection
              .write("conversation.delete", { conversation_id: task.conversation_id as string }, { retry: false })
              .then(() => {
                void client.invalidateQueries({ queryKey: ["tasks"] });
                void navigate({ to: "/tasks" });
              })
              .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
          }}
        />
      ) : null}
      <div {...stylex.props(styles.body)}>
        <section aria-label="Run" {...stylex.props(styles.run)}>
          <div {...stylex.props(styles.scroll)}>
            <div {...stylex.props(styles.thread)}>
              <div>
                <Heading level={2} xstyle={styles.taskTitle}>
                  {task.title}
                </Heading>
                <p {...stylex.props(styles.goal)}>{task.goal}</p>
              </div>
              {messages.error ? <Problem>{messages.error.message}</Problem> : null}
              {list.map((m) =>
                m.status === "partial" && task.conversation_id ? (
                  <PartialMessage
                    key={m.id}
                    m={m}
                    name={name}
                    employeeId={employeeId}
                    conversationId={task.conversation_id}
                  />
                ) : (
                  <Message key={m.id} m={m} name={name} employeeId={employeeId} conversationId={task.conversation_id} />
                ),
              )}
              {task.conversation_id ? (
                <Streaming conversationId={task.conversation_id} name={name} employeeId={employeeId} known={known} />
              ) : null}
              {pending.map((a) => (
                <ApprovalCard key={a.id} approval={a} />
              ))}
              {task.result && !isActive(task) && list.length === 0 ? <Markdown text={task.result} /> : null}
              <div ref={end} />
            </div>
          </div>
          {status === "online" ? (
            <Composer
              task={task}
              name={name}
              live={live}
              activeRun={live ? (lastRun ?? null) : null}
              run={messages.data?.run}
              waiting={!live && isActive(task)}
            />
          ) : null}
        </section>
        <RunAside label="Run properties">
          <SideProperties>
            <SideRow name="Employee">{name}</SideRow>
            <SideRow name="Status">{state.text}</SideRow>
            {project ? <SideRow name="Project">{project.name}</SideRow> : null}
            <SideRow name="Model">
              {(task.run.model ?? messages.data?.run.model)
                ? modelName((task.run.model ?? messages.data?.run.model) as string)
                : "Default"}
            </SideRow>
            <SideRow name="Mode">{task.run.mode ?? messages.data?.agent_mode ?? "Default"}</SideRow>
            {task.schedule_id ? (
              <SideRow name="From schedule">
                {hidden("RecurringTasks") ? "Recurring task" : <Link href="/settings/schedules">Recurring task</Link>}
              </SideRow>
            ) : null}
            <SideRow name="Created">
              {new Date(task.created_at_ms).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" })}
            </SideRow>
          </SideProperties>
          {subtasks.length > 0 ? (
            <SideSection title={`Subtasks (${subtasks.length})`}>
              {subtasks.map(({ task: sub, depth }) => {
                const line = taskStateLine(sub);
                return (
                  <Link
                    key={sub.id}
                    href={`/tasks/${sub.id}`}
                    color="inherit"
                    style={{ paddingLeft: 4 * (depth - 1) }}
                    xstyle={styles.subtask}
                  >
                    <span {...stylex.props(styles.subtaskTitle)}>{sub.title}</span>
                    <span {...stylex.props(styles.subtaskState)} style={{ color: line.color }}>
                      {line.text}
                    </span>
                  </Link>
                );
              })}
            </SideSection>
          ) : null}
          {task.conversation_id ? <SubagentsAtWork conversationId={task.conversation_id} live={live} /> : null}
          <FilesWritten files={files} />
          <RunGuidelines runId={lastRun} />
          <UsageTotalsLine usage={usage} />
        </RunAside>
      </div>
    </Page>
  );
}
