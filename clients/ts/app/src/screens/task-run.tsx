// A task's run: the conversation of the task with its employee. It is the employee chat, as a work
// session is, with the head of a task (project › ticket › "Run by …" and its state), the task's buttons
// and a panel with the run's properties. Messages go in with `employee.send_message`, so the server
// checks that the employee may still take them.

import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { LayoutHeader } from "@astryxdesign/core/Layout";
import { Link } from "@astryxdesign/core/Link";
import type { TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection } from "../connection";
import { useAllTasks, useMessages, useProjects } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { designPages } from "./artifact-logic";
import { ContextDialog } from "./chat-message";
import { type ChatMessage, EmployeeChat } from "./employee-chat";
import { effortName, MODE_LABELS, nextMode } from "./employee-logic";
import { Confirm, Page } from "./org-kit";
import { useRunDefaults, useTicketLogin } from "./run-default";
import { boardOnlyReason, lastRunOf, usageTotals } from "./run-logic";
import {
  DesignPagesSection,
  FilesWritten,
  type PropertiesDock,
  PropertiesPanel,
  PropertiesToggle,
  RunGuidelines,
  SideProperties,
  SideRow,
  SideSection,
  usePropertiesDock,
} from "./run-side";
import { UsageChip, useRetryAnswer } from "./run-tools";
import { RunChips } from "./session";
import { runChipLabel } from "./session-logic";
import { modelName } from "./settings-kit";
import { SubagentsAtWork } from "./subagents";
import { filesWritten, isActive, subtasksOf, taskBackPath, taskStateLine } from "./tasks-logic";

const styles = stylex.create({
  body: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0 },
  head: {
    display: "flex",
    minWidth: 0,
    alignItems: "center",
    gap: 8,
    color: "var(--precision-text3)",
  },
  bare: { minHeight: 44, paddingInlineStart: 18, paddingInlineEnd: 12 },
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
    fontWeight: 600,
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
  actions: { display: "flex", flexShrink: 0, alignItems: "center", gap: 6 },
  result: { display: "flex", flexDirection: "column", gap: 12 },
  resultLabel: { margin: 0, fontSize: 12, color: "var(--precision-text3)" },
  goal: { margin: 0, padding: 18, whiteSpace: "pre-wrap", color: "var(--color-text-secondary)" },
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

export function TaskRunScreen({ taskId, design = null }: { taskId: string; design?: string | null }) {
  useKeyContext("TaskRun");
  const navigate = useNavigate();
  const tasks = useAllTasks();
  const task = tasks.data?.find((t) => t.id === taskId);
  useAction("Back", () => void navigate({ href: taskBackPath(task) }));

  if (!task) {
    return (
      <Page>
        <LayoutHeader hasDivider padding={0} data-titlebar>
          <div {...stylex.props(styles.head, styles.bare)}>
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
  // A task nobody started yet has no conversation: its head and goal, nothing to write in.
  if (!task.conversation_id) {
    return (
      <Page>
        <LayoutHeader hasDivider padding={0} data-titlebar>
          <div {...stylex.props(styles.head, styles.bare)}>
            <TaskHead task={task} />
          </div>
        </LayoutHeader>
        <p {...stylex.props(styles.goal)}>{task.goal}</p>
      </Page>
    );
  }
  return <TaskConversation task={task} conversationId={task.conversation_id} design={design} />;
}

function TaskConversation({
  task,
  conversationId,
  design,
}: {
  task: TaskView;
  conversationId: string;
  design: string | null;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const dock = usePropertiesDock();
  const cancel = useTaskCancel();
  const messages = useMessages(conversationId);
  const list = useMemo(() => messages.data?.messages ?? [], [messages.data]);
  const lastRun = lastRunOf(list);
  const retry = useRetryAnswer(conversationId, list, { employeeId: task.employee_id ?? null });
  const [contextOf, setContextOf] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const live = Boolean(task.live_run);
  const name = task.employee_name ?? "The employee";
  const login = useTicketLogin(task.ticket_id);

  const showContext = () => {
    if (lastRun) setContextOf(lastRun);
    else toast("Open a conversation with an answer to see the context it used.");
  };
  const stop = async () => {
    if (!lastRun) return;
    try {
      await connection.write("run.stop", { run_id: lastRun }, { retry: false });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  useAction("ShowContext", showContext);
  useAction("RetryAnswer", retry);
  useAction("StopGeneration", () => void stop(), live);
  useAction("CycleAgentMode", () => {
    const mode = messages.data?.agent_mode;
    if (!mode) return;
    connection
      .write("conversation.set_agent_mode", { conversation_id: conversationId, mode: nextMode(mode) }, { retry: false })
      .then(() => client.invalidateQueries({ queryKey: ["messages", conversationId] }))
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  });

  // The server keeps an employee's conversations apart from plain chats.
  const send = (message: ChatMessage) => {
    const params = {
      conversation_id: conversationId,
      text: message.text,
      images: message.images,
      ...(message.files.length > 0 ? { files: message.files } : {}),
      confirm_secret: message.confirmSecret,
    };
    return task.employee_id
      ? connection.write("employee.send_message", { ...params, employee_id: task.employee_id }, { retry: false })
      : connection.write("chat.send", params, { retry: false });
  };

  return (
    <div {...stylex.props(styles.body)}>
      <EmployeeChat
        conversationId={conversationId}
        frame={{
          title: `Run by ${name}`,
          head: (
            <div {...stylex.props(styles.head)}>
              <TaskHead task={task} />
            </div>
          ),
          noun: "task",
          taskId: task.id,
          design,
          activeRun: live ? (lastRun ?? null) : null,
          send,
          empty: { title: task.title, description: task.goal },
          ...(task.result && !isActive(task)
            ? {
                emptyView: (
                  <div {...stylex.props(styles.result)}>
                    <p {...stylex.props(styles.resultLabel)}>Result</p>
                    <Markdown text={task.result} />
                  </div>
                ),
              }
            : {}),
          footer: (snapshot) => <RunChips snapshot={snapshot} login={login} />,
          actions: (
            <div {...stylex.props(styles.actions)}>
              <UsageChip taskId={task.id} />
              <Menu
                label="Conversation"
                options={[
                  { key: "context", label: "Show context of last answer" },
                  { key: "delete", label: "Delete conversation…", danger: true },
                ]}
                onPick={(key) => (key === "delete" ? setDeleting(true) : showContext())}
                trigger={<Button label="Conversation">⋯</Button>}
              />
              {live && lastRun ? (
                <Button label="Stop run" onClick={() => void stop()} />
              ) : isActive(task) && boardOnlyReason(task) === null ? (
                <Button variant="destructive" label="Cancel task" onClick={() => void cancel(task)} />
              ) : null}
              <PropertiesToggle dock={dock} label="Run properties" />
            </div>
          ),
          aside: <TaskDetails dock={dock} task={task} conversationId={conversationId} />,
        }}
      />
      {contextOf ? <ContextDialog run={contextOf} onClose={() => setContextOf(null)} /> : null}
      {deleting ? (
        <Confirm
          danger
          title="Delete this conversation?"
          body="Its messages are gone for good. The task itself stays."
          action="Delete"
          onClose={() => setDeleting(false)}
          onConfirm={() => {
            setDeleting(false);
            connection
              .write("conversation.delete", { conversation_id: conversationId }, { retry: false })
              .then(() => {
                void client.invalidateQueries({ queryKey: ["tasks"] });
                void navigate({ href: taskBackPath(task) });
              })
              .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
          }}
        />
      ) : null}
    </div>
  );
}

/** Where the task belongs (project › ticket), who runs it and its state. */
function TaskHead({ task }: { task: TaskView }) {
  const projects = useProjects();
  const project = projects.data?.find((p) => p.id === task.project_id);
  const state = taskStateLine(task);
  return (
    <>
      {project ? (
        <>
          <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={4} />
          <span {...stylex.props(styles.truncate)}>{project.name}</span>
          <span {...stylex.props(styles.chevron)}>›</span>
        </>
      ) : null}
      {task.ticket_id ? (
        <>
          <Link
            href={task.ticket_number ? `/ticket/${task.ticket_number}` : `/board?ticket=${task.ticket_id}`}
            color="inherit"
            xstyle={styles.ticketLink}
          >
            #{task.ticket_number}
          </Link>
          <span {...stylex.props(styles.chevron)}>›</span>
        </>
      ) : null}
      <Heading level={1} xstyle={styles.title}>
        Run by {task.employee_name ?? "The employee"}
      </Heading>
      <span {...stylex.props(styles.state)} style={{ color: state.color }}>
        {state.text}
      </span>
    </>
  );
}

/** The run's properties panel: the same as a session's, with the task's own rows and its subtasks. */
function TaskDetails({ dock, task, conversationId }: { dock: PropertiesDock; task: TaskView; conversationId: string }) {
  const tasks = useAllTasks();
  const projects = useProjects();
  const feed = useMessages(conversationId);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  const files = useMemo(() => filesWritten(list), [list]);
  const designs = useMemo(() => designPages(list), [list]);
  const usage = useMemo(() => usageTotals(list), [list]);
  const subtasks = useMemo(() => subtasksOf(tasks.data ?? [], task.id), [tasks.data, task.id]);
  const project = projects.data?.find((p) => p.id === task.project_id);
  const name = task.employee_name ?? "The employee";
  const model = feed.data?.run.model ?? task.run.model;
  const effort = feed.data?.run.effort ?? task.run.effort;
  const mode = feed.data?.agent_mode;
  const defaults = useRunDefaults(useTicketLogin(task.ticket_id));
  return (
    <PropertiesPanel dock={dock} label="Run properties">
      <SideProperties usage={usage}>
        <SideRow name="Employee">{name}</SideRow>
        <SideRow name="Status">{taskStateLine(task).text}</SideRow>
        {project ? <SideRow name="Project">{project.name}</SideRow> : null}
        <SideRow name="Model">{runChipLabel(model, modelName, defaults?.model)}</SideRow>
        <SideRow name="Effort">{runChipLabel(effort, effortName, defaults?.effort)}</SideRow>
        {mode ? <SideRow name="Mode">{MODE_LABELS[mode]}</SideRow> : null}
        {task.schedule_id ? <SideRow name="From schedule">Recurring task</SideRow> : null}
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
      <SubagentsAtWork conversationId={conversationId} live={Boolean(task.live_run)} askedBy={name} />
      <DesignPagesSection pages={designs} />
      <FilesWritten files={files} />
      <RunGuidelines runId={lastRunOf(list)} />
    </PropertiesPanel>
  );
}
