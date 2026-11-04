import type { StreamUpdate } from "@aurelius/client";
import type { ActivityView, MessageView, TaskRun, TaskView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useApprovals, useMessages, useProjects, useRunGuidelines } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { AttachedImages, useImagePaste } from "../ui/attach";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { ApprovalCard } from "./approval-card";
import { ContextDialog } from "./chat-message";
import { Held, useModels, useSend } from "./dialogs";
import { nextMode } from "./employee-logic";
import { Confirm } from "./org-kit";
import { boardOnlyReason } from "./run-logic";
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
import { filesWritten, isActive, stepLine, stepsOf, taskStateLine } from "./tasks-logic";
import { tokensShort } from "./usage-logic";

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

function Steps({ activity }: { activity: ActivityView[] }) {
  const [open, setOpen] = useState(false);
  const steps = stepsOf(activity);
  if (!steps) return null;
  return (
    <div className="overflow-hidden rounded-[6px] border border-line bg-sunk">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex h-8 w-full cursor-pointer items-center gap-[10px] border-0 bg-transparent px-[10px] text-left text-[12px] text-text2"
      >
        <span aria-hidden="true">{open ? "⌄" : "›"}</span>
        <span className="font-medium text-text">
          {steps.count} {steps.count === 1 ? "step" : "steps"}
        </span>
        {steps.failed > 0 ? <span className="text-red">{steps.failed} failed</span> : null}
        <span className="min-w-0 flex-1 truncate font-mono text-text3">{steps.last}</span>
      </button>
      {open ? (
        <ul className="m-0 flex max-h-[320px] list-none flex-col gap-1 overflow-y-auto border-t border-line2 p-[10px]">
          {activity.map((a) => (
            <li
              key={`${a.call_id}${a.offset}`}
              className={`truncate font-mono text-[12px] ${a.ok === false ? "text-red" : "text-text2"}`}
            >
              {stepLine(a)}
            </li>
          ))}
        </ul>
      ) : null}
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
  if (m.system) return <div className="text-center text-[12px] text-text3">{m.text}</div>;
  if (m.role === "user") {
    return (
      <div className="flex gap-3">
        <Mark letter={initial(m.author_user?.username ?? "You")} color="#55595F" size={22} radius={11} />
        <div className="min-w-0 flex-1 rounded-[8px] border border-line bg-sunk px-3 py-[10px]">
          <div className="flex items-baseline gap-2">
            <span className="font-medium">{m.author_user?.username ?? "You"}</span>
            <span className="text-[12px] text-text3">{m.queued ? "Queued" : clock(m.created_at_ms)}</span>
          </div>
          <div className="mt-[6px]">
            <Markdown text={m.text} />
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="flex gap-3">
      <Mark letter={initial(name)} color={markColor(employeeId)} size={22} radius={6} />
      <div className="flex min-w-0 flex-1 flex-col gap-[10px]">
        <div className="flex items-baseline gap-2">
          <span className="font-medium">{name}</span>
          <span className="text-[12px] text-text3">{clock(m.created_at_ms)}</span>
          {m.status === "failed" ? (
            <span className="text-[12px] text-red">Failed</span>
          ) : m.status === "interrupted" ? (
            <span className="text-[12px] text-text3">Stopped</span>
          ) : null}
        </div>
        <Steps activity={m.activity} />
        {m.text ? <Markdown text={m.text} /> : null}
        <MarkdownChips
          text={m.text}
          scope={conversationId ? { kind: "conversation", conversation_id: conversationId } : null}
        />
        <FileChips files={m.uploads} />
        <KeptFiles files={m.files ?? []} />
        {m.run_id && m.status !== "partial" ? (
          <TraceLink
            run={m.run_id}
            className="cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-text3 hover:text-text"
          />
        ) : null}
      </div>
    </div>
  );
});

/** Only this re-renders while a run streams; the messages above it stay put. */
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
  const connection = useConnection();
  const [update, setUpdate] = useState<StreamUpdate | null>(null);
  useEffect(
    () => connection.streams.on((u) => u.conversation_id === conversationId && setUpdate(u)),
    [connection, conversationId],
  );
  if (!update || known.has(update.message_id)) return null;
  return (
    <div className="flex gap-3">
      <Mark letter={initial(name)} color={markColor(employeeId)} size={22} radius={6} />
      <div className="flex min-w-0 flex-1 flex-col gap-[10px]">
        <div className="flex items-baseline gap-2">
          <span className="font-medium">{name}</span>
          <span className="text-[12px] text-acc-text">Working…</span>
        </div>
        <Steps activity={update.activity} />
        {update.text ? <p className="m-0 text-[14px] leading-[1.65] whitespace-pre-wrap">{update.text}</p> : null}
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
  const { busy, held, error, send, sendAnyway } = useSend(() => {
    setText("");
    void client.invalidateQueries({ queryKey: ["messages", task.conversation_id] });
  });
  const go = () => {
    const conversationId = task.conversation_id;
    if (!conversationId || text.trim() === "" || busy) return;
    send(async (confirmSecret) => {
      await connection.write("chat.send", {
        conversation_id: conversationId,
        text: text.trim(),
        confirm_secret: confirmSecret,
        ...(images.paths.length > 0 ? { images: images.paths } : {}),
      });
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
    <div className="mx-auto flex w-full max-w-[760px] flex-col gap-2 px-10 pb-[18px] max-[900px]:px-4">
      <LoadBanner waiting={waiting} />
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? (
        <p role="alert" className="m-0 text-red">
          {error}
        </p>
      ) : null}
      <div {...images.target} className="rounded-[10px] border border-btn-line bg-sunk">
        <AttachedImages images={images} />
        <textarea
          ref={area}
          aria-label={`Message ${name}`}
          rows={2}
          placeholder={`Message ${name}`}
          className="block w-full resize-none border-0 bg-transparent px-[14px] py-3 text-[14px] text-text outline-none placeholder:text-text3"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
              e.preventDefault();
              go();
            }
          }}
        />
        <div className="flex items-center gap-[6px] px-2 pb-2">
          {run ? <RunMenu conversationId={task.conversation_id} run={run} models={models} /> : null}
          <span className="ml-auto text-[12px] text-text3">{live ? "Queued until the step finishes" : ""}</span>
          <Button kind="primary" disabled={busy || text.trim() === ""} onClick={go}>
            Send
          </Button>
        </div>
      </div>
    </div>
  );
}

function Row({ name, children }: { name: string; children: React.ReactNode }) {
  return (
    <div className="grid min-h-[30px] grid-cols-[100px_minmax(0,1fr)] items-center">
      <span className="text-text3">{name}</span>
      <span className="min-w-0 truncate">{children}</span>
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
  const known = useMemo(() => new Set(list.filter((m) => m.status !== "partial").map((m) => m.id)), [list]);
  const lastRun = [...list].reverse().find((m) => m.run_id)?.run_id;
  const guidelines = useRunGuidelines(lastRun);
  const files = useMemo(() => filesWritten(list), [list]);
  const usage = useMemo(() => {
    const totals = { input: 0, output: 0, cached: 0, written: 0, micro: 0 };
    for (const m of list) {
      totals.input += m.usage?.input_tokens ?? 0;
      totals.output += m.usage?.output_tokens ?? 0;
      totals.cached += m.usage?.cached_tokens ?? 0;
      totals.written += m.usage?.cache_creation_tokens ?? 0;
      totals.micro += m.usage?.cost_micro_usd ?? 0;
    }
    return totals;
  }, [list]);
  const end = useRef<HTMLDivElement>(null);
  const count = list.length;
  useEffect(() => {
    if (count > 0) end.current?.scrollIntoView({ block: "end" });
  }, [count]);
  useAction("Back", () => void navigate({ to: "/tasks" }));
  const retry = useRetryAnswer(task?.conversation_id, list);
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
      <>
        <header className="flex h-11 flex-none items-center border-b border-line pl-[18px]">
          <h1 className="m-0 text-[13px] font-medium">Task</h1>
        </header>
        <p className="m-0 p-[18px] text-text3">
          {tasks.error?.message ?? (tasks.isPending ? "Loading the task…" : "This task is not on this server.")}
        </p>
      </>
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
    <>
      <header className="flex h-11 flex-none items-center gap-2 border-b border-line pr-3 pl-[18px] text-text3">
        {project ? (
          <>
            <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={4} />
            <span className="truncate">{project.name}</span>
            <span className="text-faint">›</span>
          </>
        ) : null}
        {task.ticket_id ? (
          <>
            <Link
              to="/board"
              search={{ ticket: task.ticket_id }}
              className="truncate text-text2 no-underline hover:text-text"
            >
              #{task.ticket_number}
            </Link>
            <span className="text-faint">›</span>
          </>
        ) : null}
        <h1 className="m-0 truncate text-[13px] font-normal text-text">Run by {name}</h1>
        <span
          className="ml-2 inline-flex h-[22px] flex-none items-center gap-[6px] rounded-[11px] border border-line px-[9px] text-[12px] font-medium"
          style={{ color: state.color }}
        >
          {state.text}
        </span>
        <div className="ml-auto flex flex-none items-center gap-[6px]">
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
            trigger={<Button>⋯</Button>}
          />
          {!live && lastAnswer && (lastAnswer.status === "failed" || lastAnswer.status === "interrupted") ? (
            <Button onClick={retry}>Retry answer</Button>
          ) : null}
          {live && lastRun ? (
            <Button onClick={() => void stop()}>Stop run</Button>
          ) : isActive(task) && boardOnlyReason(task) === null ? (
            <Button kind="danger" onClick={() => void cancel(task)}>
              Cancel task
            </Button>
          ) : null}
        </div>
      </header>
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
      <div className="flex min-h-0 flex-1">
        <section aria-label="Run" className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto flex w-full max-w-[760px] flex-col gap-5 px-10 pt-7 pb-4 max-[900px]:px-4">
              <div>
                <h2 className="m-0 text-[22px] font-semibold tracking-[-0.015em]">{task.title}</h2>
                <p className="mt-[6px] mb-0 text-[14px] leading-[1.6] whitespace-pre-wrap text-text2">{task.goal}</p>
              </div>
              {messages.error ? (
                <p role="alert" className="m-0 text-red">
                  {messages.error.message}
                </p>
              ) : null}
              {list.map((m) => (
                <Message key={m.id} m={m} name={name} employeeId={employeeId} conversationId={task.conversation_id} />
              ))}
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
        <aside
          aria-label="Run properties"
          className="flex w-[clamp(240px,24%,340px)] flex-none flex-col gap-5 overflow-y-auto border-l border-line bg-sunk p-[18px] max-[900px]:hidden"
        >
          <div>
            <p className="mt-0 mb-2 text-[12px] font-medium text-text3">Properties</p>
            <Row name="Employee">{name}</Row>
            <Row name="Status">{state.text}</Row>
            {project ? <Row name="Project">{project.name}</Row> : null}
            <Row name="Model">
              {(task.run.model ?? messages.data?.run.model)
                ? modelName((task.run.model ?? messages.data?.run.model) as string)
                : "Default"}
            </Row>
            <Row name="Mode">{task.run.mode ?? messages.data?.agent_mode ?? "Default"}</Row>
            {task.schedule_id ? (
              <Row name="From schedule">
                <Link to="/settings/$section" params={{ section: "schedules" }} className="text-acc-text no-underline">
                  Recurring task
                </Link>
              </Row>
            ) : null}
            <Row name="Created">
              {new Date(task.created_at_ms).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" })}
            </Row>
          </div>
          {task.conversation_id ? <SubagentsAtWork conversationId={task.conversation_id} live={live} /> : null}
          {files.length > 0 ? (
            <div className="border-t border-line2 pt-4">
              <p className="mt-0 mb-2 text-[12px] font-medium text-text3">Files written</p>
              {files.map((f) => (
                <div key={f} title={f} className="truncate py-1 font-mono text-[12px] text-text2">
                  {f}
                </div>
              ))}
            </div>
          ) : null}
          {(guidelines.data?.guidelines ?? []).length > 0 ? (
            <div className="border-t border-line2 pt-4">
              <p className="mt-0 mb-2 text-[12px] font-medium text-text3">Guidelines in this run</p>
              {guidelines.data?.guidelines.map((g) => (
                <div key={g.guideline_id} className="flex justify-between gap-2 py-1">
                  <span className="truncate">{g.title}</span>
                  <span className="flex-none text-text3">v{g.version}</span>
                </div>
              ))}
            </div>
          ) : null}
          {usage.input + usage.output > 0 ? (
            <div className="border-t border-line2 pt-4 text-[12px] leading-[1.7] text-text3">
              {tokensShort(usage.input)} in · {tokensShort(usage.output)} out
              <br />
              {tokensShort(usage.cached)} cache read · {tokensShort(usage.written)} write
              {usage.micro > 0 ? (
                <>
                  <br />≈ ${(usage.micro / 1_000_000).toFixed(2)} API-equivalent
                </>
              ) : null}
            </div>
          ) : null}
        </aside>
      </div>
    </>
  );
}
