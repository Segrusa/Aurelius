import type { TaskView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useConnection } from "../connection";
import { useAllTasks, useEmployees, useMessages, useProjects } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { Button, initial, Kbd, Mark, markColor } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { overlays } from "./overlays-store";
import { boardOnlyReason } from "./run-logic";
import { useTaskCancel } from "./task-run";
import {
  admits,
  filterTasks,
  foldTree,
  isActive,
  stepLine,
  TASK_STATES,
  type TaskState,
  taskStateLine,
  treeOrder,
} from "./tasks-logic";
import { ago } from "./ticket-rows";

const ROW_HEIGHT = 52;

/** Who else may do a task that is not running; a ticket's main task goes through the board. */
function useAssign(task: TaskView | undefined) {
  const connection = useConnection();
  const client = useQueryClient();
  const employees = useEmployees();
  const options = (employees.data ?? [])
    .filter((e) => e.status === "active" && e.id !== task?.employee_id)
    .map((e) => ({ key: e.id, label: e.name }));
  const pick = async (employeeId: string) => {
    if (!task) return;
    try {
      await connection.write("task.assign", { task_id: task.id, employee_id: employeeId }, { retry: false });
      await client.invalidateQueries({ queryKey: ["tasks"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const waiting = task ? ASSIGNABLE.includes(task.status) : false;
  const allowed = task !== undefined && boardOnlyReason(task) === null && waiting;
  return { options, pick, allowed };
}

const ASSIGNABLE: TaskView["status"][] = ["draft", "queued", "waiting"];

function Preview({ task, onOpen }: { task: TaskView; onOpen: () => void }) {
  const messages = useMessages(task.conversation_id);
  const cancel = useTaskCancel();
  const assign = useAssign(task);
  const [giving, setGiving] = useState(false);
  useAction("AssignSelectedTask", () => {
    const why = boardOnlyReason(task);
    if (why) toast(why);
    else if (!assign.allowed) toast("A task can only be given to another employee while it is not running.");
    else setGiving(true);
  });
  useAction("CancelSelectedTask", () => {
    const why = boardOnlyReason(task);
    if (why) toast(why);
    else if (isActive(task)) void cancel(task);
  });
  useAction("OpenTaskConversation", onOpen);
  const state = taskStateLine(task);
  const last = [...(messages.data?.messages ?? [])].reverse().find((m) => m.role === "assistant");
  const step = last?.activity.at(-1);
  return (
    <div key={task.id} className="enter flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-[22px] py-5">
      <div className="flex items-center gap-2 text-[12px] text-text3">
        {task.ticket_number ? <span className="font-mono">#{task.ticket_number}</span> : null}
        {task.delegated_by ? <span>· delegated</span> : null}
        <span className="ml-auto inline-flex items-center gap-[6px]" style={{ color: state.color }}>
          <span className="size-[7px] rounded-full" style={{ background: state.color }} />
          {state.text}
        </span>
      </div>
      <h2 className="m-0 text-[18px] leading-[1.35] font-semibold tracking-[-0.01em]">{task.title}</h2>
      {task.employee_name ? (
        <div className="flex items-center gap-2">
          <Mark letter={initial(task.employee_name)} color={markColor(task.employee_id ?? "")} size={20} radius={5} />
          {task.employee_name}
        </div>
      ) : null}
      {last ? (
        <div className="flex flex-col gap-2 rounded-[8px] border border-line bg-sunk p-3">
          <div className="text-[12px] font-medium text-text3">{isActive(task) ? "Now" : "Last"}</div>
          {step ? <div className="truncate font-mono text-[12px] text-text2">{stepLine(step)}</div> : null}
          {last.text ? <p className="m-0 line-clamp-4 text-[13px] leading-[1.6]">{last.text}</p> : null}
        </div>
      ) : null}
      <div>
        <div className="mb-[6px] text-[12px] font-medium text-text3">Goal</div>
        <p className="m-0 text-[13px] leading-[1.6] whitespace-pre-wrap text-text2">{task.goal}</p>
      </div>
      {task.result ? (
        <div>
          <div className="mb-[6px] text-[12px] font-medium text-text3">Result</div>
          <Markdown text={task.result} />
        </div>
      ) : null}
      <div className="mt-auto flex gap-2">
        <Button onClick={onOpen}>
          Open conversation <Kbd>↵</Kbd>
        </Button>
        {assign.allowed ? (
          <Menu
            label="Give to"
            open={giving}
            onOpenChange={setGiving}
            options={assign.options}
            onPick={(id) => void assign.pick(id)}
            trigger={<Button>Give to…</Button>}
          />
        ) : null}
        {isActive(task) && boardOnlyReason(task) === null ? (
          <Button kind="danger" onClick={() => void cancel(task)}>
            Cancel task
          </Button>
        ) : null}
      </div>
    </div>
  );
}

export function TasksScreen({ projectId }: { projectId?: string } = {}) {
  useKeyContext("Tasks");
  const navigate = useNavigate();
  const tasks = useAllTasks();
  const employees = useEmployees();
  const projects = useProjects();
  const [state, setState] = useState<TaskState>("open");
  const [employee, setEmployee] = useState<string | null>(null);
  const [chosenProject, setProject] = useState<string | null>(null);
  const project = projectId ?? chosenProject;
  const [folds, setFolds] = useState<Record<string, boolean>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const list = tasks.data ?? [];
  const rows = useMemo(
    () => foldTree(treeOrder(filterTasks(tasks.data ?? [], { state, employee, project })), folds),
    [tasks.data, state, employee, project, folds],
  );
  const current = rows.find((r) => r.task.id === selected) ?? rows[0];
  const at = current ? rows.indexOf(current) : -1;
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });
  useEffect(() => {
    if (at >= 0) virtual.scrollToIndex(at, { align: "auto" });
  }, [at, virtual]);
  const openRun = (id: string) => void navigate({ to: "/tasks/$taskId", params: { taskId: id } });

  const step = (by: number) => {
    const next = rows[at + by];
    if (next) setSelected(next.task.id);
  };
  useAction("SelectNext", () => step(1));
  useAction("SelectPrev", () => step(-1));
  useAction("OpenSelected", () => current && openRun(current.task.id));
  useAction("Filter1", () => setState("open"));
  useAction("Filter2", () => setState("needs_you"));
  useAction("Filter3", () => setState("ended"));
  useAction("Filter4", () => setState("all"));

  const pill =
    "h-6 flex-none cursor-pointer rounded-[12px] border border-dashed whitespace-nowrap border-btn-line bg-transparent px-[9px] text-[12px] font-medium text-text2 hover:text-text";
  const employeeName = employees.data?.find((e) => e.id === employee)?.name ?? "Anyone";
  const projectName = projects.data?.find((p) => p.id === project)?.name ?? "Any project";

  return (
    <>
      <header className="flex h-11 flex-none items-center gap-[10px] border-b border-line pr-3 pl-[18px]">
        <h1 className="m-0 text-[13px] font-medium">Tasks</h1>
        <fieldset aria-label="State" className="m-0 ml-2 flex gap-1 border-0 p-0">
          {TASK_STATES.map(({ key, label }) => (
            <button
              key={key}
              type="button"
              aria-pressed={key === state}
              onClick={() => setState(key)}
              className={`h-6 cursor-pointer rounded-[5px] border px-[9px] text-[12px] font-medium ${key === state ? "border-btn-line bg-sel text-text" : "border-transparent bg-transparent text-text2 hover:text-text"}`}
            >
              {label} <span className="text-text3">{list.filter((t) => admits(key, t)).length}</span>
            </button>
          ))}
        </fieldset>
        <Menu
          label="Employee"
          options={[
            { key: "any", label: "Anyone" },
            ...(employees.data ?? []).map((e) => ({ key: e.id, label: e.name })),
          ]}
          onPick={(id) => setEmployee(id === "any" ? null : id)}
          trigger={
            <button type="button" className={pill}>
              {employeeName}
            </button>
          }
        />
        {projectId ? null : (
          <Menu
            label="Project"
            options={[
              { key: "any", label: "Any project" },
              ...(projects.data ?? []).map((p) => ({ key: p.id, label: p.name })),
            ]}
            onPick={(id) => setProject(id === "any" ? null : id)}
            trigger={
              <button type="button" className={pill}>
                {projectName}
              </button>
            }
          />
        )}
        <div className="ml-auto">
          <Button kind="primary" onClick={() => overlays.open("give")}>
            Give work <Kbd onAccent>N</Kbd>
          </Button>
        </div>
      </header>
      <div className="flex min-h-0 flex-1">
        <section aria-label="Task list" className="flex min-w-0 flex-[3] flex-col border-r border-line">
          {tasks.error ? (
            <p role="alert" className="m-0 p-[18px] text-red">
              {tasks.error.message}
            </p>
          ) : rows.length === 0 && !tasks.isPending ? (
            <p className="m-0 p-[18px] text-text3">No tasks here.</p>
          ) : null}
          <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
            <div className="relative" style={{ height: virtual.getTotalSize() }}>
              {virtual.getVirtualItems().map((item) => {
                const row = rows[item.index];
                if (!row) return null;
                const { task, depth, below, open } = row;
                const look = taskStateLine(task);
                const meta = [
                  task.ticket_number ? `#${task.ticket_number}` : null,
                  below > 0 ? `${below} below` : null,
                  task.delegated_by ? "delegated" : null,
                ]
                  .filter(Boolean)
                  .join(" · ");
                const on = task.id === current?.task.id;
                return (
                  <div
                    key={task.id}
                    className={`absolute inset-x-0 flex items-center border-b border-line2 ${on ? "bg-sel shadow-[inset_2px_0_0_var(--acc-text)]" : ""}`}
                    style={{
                      height: ROW_HEIGHT,
                      transform: `translateY(${item.start}px)`,
                      paddingLeft: 14 + depth * 22,
                    }}
                  >
                    {below > 0 ? (
                      <button
                        type="button"
                        aria-label={open ? "Fold" : "Unfold"}
                        aria-expanded={open}
                        onClick={() => setFolds({ ...folds, [task.id]: !open })}
                        className="grid size-5 flex-none cursor-pointer place-items-center rounded-[4px] border-0 bg-transparent text-text3 hover:text-text"
                      >
                        {open ? "⌄" : "›"}
                      </button>
                    ) : (
                      <span className="w-5 flex-none" />
                    )}
                    <button
                      type="button"
                      aria-current={on ? "true" : undefined}
                      onClick={() => setSelected(task.id)}
                      onDoubleClick={() => openRun(task.id)}
                      className="flex h-full min-w-0 flex-1 cursor-pointer items-center gap-3 border-0 bg-transparent pr-[18px] pl-1 text-left text-text"
                    >
                      {task.employee_name ? (
                        <Mark
                          letter={initial(task.employee_name)}
                          color={markColor(task.employee_id ?? "")}
                          size={22}
                          radius={6}
                        />
                      ) : (
                        <span className="size-[22px] flex-none" />
                      )}
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium">{task.title}</span>
                        <span className="mt-[2px] block truncate text-[12px] text-text3">
                          {[task.employee_name, meta].filter(Boolean).join(" · ")}
                        </span>
                      </span>
                      <span
                        className="inline-flex items-center gap-[6px] text-[12px] whitespace-nowrap"
                        style={{ color: look.color }}
                      >
                        <span className="size-[7px] rounded-full" style={{ background: look.color }} />
                        {look.text}
                      </span>
                      <span className="w-10 flex-none text-right text-[12px] text-text3">
                        {ago(task.updated_at_ms)}
                      </span>
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        </section>
        <aside aria-label="Task preview" className="flex w-[clamp(320px,36%,520px)] min-w-0 flex-none flex-col">
          {current ? (
            <Preview key={current.task.id} task={current.task} onOpen={() => openRun(current.task.id)} />
          ) : null}
        </aside>
      </div>
    </>
  );
}
