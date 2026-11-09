import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import type { TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useConnection } from "../connection";
import { useAllTasks, useEmployees, useMessages } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { KeyHint } from "../ui/key-hint";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { Loading, Page, PageHead, Problem } from "./org-kit";
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

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  tabCount: { color: "var(--precision-text3)" },
  pill: {
    height: 24,
    flexShrink: 0,
    cursor: "pointer",
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "dashed",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "transparent",
    paddingInline: 9,
    fontSize: 12,
    fontWeight: 500,
    whiteSpace: "nowrap",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  body: { display: "flex", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  list: {
    display: "flex",
    minWidth: 0,
    flexGrow: 3,
    flexShrink: 1,
    flexBasis: "0%",
    flexDirection: "column",
    borderRightWidth: 1,
    borderRightStyle: "solid",
    borderRightColor: "var(--color-border)",
  },
  message: { padding: 18 },
  scroller: { minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", overflowY: "auto" },
  track: { position: "relative" },
  row: {
    position: "absolute",
    insetInline: 0,
    top: 0,
    boxSizing: "border-box",
    height: 52,
    borderRadius: 0,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
  },
  fold: {
    display: "grid",
    width: 20,
    height: 20,
    flexShrink: 0,
    cursor: "pointer",
    placeItems: "center",
    borderRadius: 4,
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  foldGap: { width: 20, flexShrink: 0 },
  start: { display: "flex", alignItems: "center", gap: 8 },
  noMark: { width: 22, height: 22, flexShrink: 0 },
  end: { display: "flex", alignItems: "center", gap: 12 },
  state: { display: "inline-flex", alignItems: "center", gap: 6, fontSize: 12, whiteSpace: "nowrap" },
  stateDot: { width: 7, height: 7, borderRadius: "50%" },
  age: { width: 40, flexShrink: 0, textAlign: "right", fontSize: 12, color: "var(--precision-text3)" },
  aside: {
    display: "flex",
    width: "clamp(320px, 36%, 520px)",
    minWidth: 0,
    flexShrink: 0,
    flexDirection: "column",
  },
  preview: {
    display: "flex",
    minHeight: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    flexDirection: "column",
    gap: 16,
    overflowY: "auto",
    paddingInline: 22,
    paddingBlock: 20,
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
  },
  previewHead: { display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--precision-text3)" },
  mono: { fontFamily: "var(--font-family-code)" },
  previewState: { marginInlineStart: "auto", display: "inline-flex", alignItems: "center", gap: 6 },
  title: { margin: 0, fontSize: 18, lineHeight: 1.35, fontWeight: 600, letterSpacing: "-0.01em" },
  employee: { display: "flex", alignItems: "center", gap: 8 },
  last: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    padding: 12,
  },
  label: { fontSize: 12, fontWeight: 500, color: "var(--precision-text3)" },
  labelGap: { marginBottom: 6 },
  step: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  lastText: {
    margin: 0,
    display: "-webkit-box",
    overflow: "hidden",
    fontSize: 13,
    lineHeight: 1.6,
    WebkitBoxOrient: "vertical",
    WebkitLineClamp: 4,
  },
  goal: {
    margin: 0,
    fontSize: 13,
    lineHeight: 1.6,
    whiteSpace: "pre-wrap",
    color: "var(--color-text-secondary)",
  },
  actions: { marginTop: "auto", display: "flex", gap: 8 },
});

const ROW_HEIGHT = 52;

/** Who else may do a task that is not running; a ticket's main task goes through the board. */
function useAssign(task: TaskView | undefined) {
  const connection = useConnection();
  const client = useQueryClient();
  const employees = useEmployees();
  const options = (employees.data ?? [])
    .filter((e) => e.status === "active" && e.id !== task?.employee_id)
    .map((e) => ({ key: e.id, label: e.name }))
    .concat(task?.employee_id ? [{ key: "draft", label: "Nobody: a draft again" }] : []);
  const pick = async (choice: string) => {
    if (!task) return;
    const employeeId = choice === "draft" ? null : choice;
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
    <div key={task.id} {...stylex.props(styles.preview)}>
      <div {...stylex.props(styles.previewHead)}>
        {task.ticket_number ? <span {...stylex.props(styles.mono)}>#{task.ticket_number}</span> : null}
        {task.delegated_by ? <span>· delegated</span> : null}
        <span {...stylex.props(styles.previewState)} style={{ color: state.color }}>
          <span {...stylex.props(styles.stateDot)} style={{ background: state.color }} />
          {state.text}
        </span>
      </div>
      <Heading level={2} xstyle={styles.title}>
        {task.title}
      </Heading>
      {task.employee_name ? (
        <div {...stylex.props(styles.employee)}>
          <Mark letter={initial(task.employee_name)} color={markColor(task.employee_id ?? "")} size={20} radius={5} />
          {task.employee_name}
        </div>
      ) : null}
      {last ? (
        <div {...stylex.props(styles.last)}>
          <div {...stylex.props(styles.label)}>{isActive(task) ? "Now" : "Last"}</div>
          {step ? <div {...stylex.props(styles.step)}>{stepLine(step)}</div> : null}
          {last.text ? <p {...stylex.props(styles.lastText)}>{last.text}</p> : null}
        </div>
      ) : null}
      <div>
        <div {...stylex.props(styles.label, styles.labelGap)}>Goal</div>
        <p {...stylex.props(styles.goal)}>{task.goal}</p>
      </div>
      {task.result ? (
        <div>
          <div {...stylex.props(styles.label, styles.labelGap)}>Result</div>
          <Markdown text={task.result} />
        </div>
      ) : null}
      <div {...stylex.props(styles.actions)}>
        <Button label="Open conversation" onClick={onOpen}>
          Open conversation <KeyHint>↵</KeyHint>
        </Button>
        {assign.allowed ? (
          <Menu
            label="Give to"
            open={giving}
            onOpenChange={setGiving}
            options={assign.options}
            onPick={(id) => void assign.pick(id)}
            trigger={<Button label="Give to…" />}
          />
        ) : null}
        {isActive(task) && boardOnlyReason(task) === null ? (
          <Button variant="destructive" label="Cancel task" onClick={() => void cancel(task)} />
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
  const [state, setState] = useState<TaskState>("open");
  const [employee, setEmployee] = useState<string | null>(null);
  const workspace = useWorkspace();
  const project = projectId ?? workspace.id;
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

  const employeeName = employees.data?.find((e) => e.id === employee)?.name ?? "Anyone";

  return (
    <Page>
      <PageHead
        title="Tasks"
        tabs={
          <>
            <ToggleButtonGroup
              label="State"
              type="single"
              size="sm"
              value={state}
              onChange={(next) => {
                if (next) setState(next as TaskState);
              }}
            >
              {TASK_STATES.map(({ key, label }) => (
                <ToggleButton key={key} value={key} label={label}>
                  {label} <span {...stylex.props(styles.tabCount)}>{list.filter((t) => admits(key, t)).length}</span>
                </ToggleButton>
              ))}
            </ToggleButtonGroup>
            <Menu
              label="Employee"
              options={[
                { key: "any", label: "Anyone" },
                ...(employees.data ?? []).map((e) => ({ key: e.id, label: e.name })),
              ]}
              onPick={(id) => setEmployee(id === "any" ? null : id)}
              trigger={
                <button type="button" {...stylex.props(styles.pill)}>
                  {employeeName}
                </button>
              }
            />
          </>
        }
      >
        <Button variant="primary" label="Give work" onClick={() => overlays.open("give")}>
          Give work <KeyHint onAccent>N</KeyHint>
        </Button>
      </PageHead>
      <div {...stylex.props(styles.body)}>
        <section aria-label="Task list" {...stylex.props(styles.list)}>
          {tasks.error ? (
            <div {...stylex.props(styles.message)}>
              <Problem>{tasks.error.message}</Problem>
            </div>
          ) : tasks.isPending && rows.length === 0 ? (
            <Loading rows={6} />
          ) : rows.length === 0 ? (
            <div {...stylex.props(styles.message)}>
              <EmptyState isCompact title="No tasks here." />
            </div>
          ) : null}
          <div ref={scroller} {...stylex.props(styles.scroller)}>
            <List density="compact" edge="flush" xstyle={styles.track} style={{ height: virtual.getTotalSize() }}>
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
                  <ListItem
                    key={task.id}
                    xstyle={styles.row}
                    style={{ transform: `translateY(${item.start}px)`, paddingLeft: 14 + depth * 22 }}
                    label={task.title}
                    description={[task.employee_name, meta].filter(Boolean).join(" · ")}
                    isSelected={on}
                    onClick={() => setSelected(task.id)}
                    onDoubleClick={() => openRun(task.id)}
                    startContent={
                      <span {...stylex.props(styles.start)}>
                        {below > 0 ? (
                          <button
                            type="button"
                            aria-label={open ? "Fold" : "Unfold"}
                            aria-expanded={open}
                            onClick={() => setFolds({ ...folds, [task.id]: !open })}
                            {...stylex.props(styles.fold)}
                          >
                            {open ? "⌄" : "›"}
                          </button>
                        ) : (
                          <span {...stylex.props(styles.foldGap)} />
                        )}
                        {task.employee_name ? (
                          <Mark
                            letter={initial(task.employee_name)}
                            color={markColor(task.employee_id ?? "")}
                            size={22}
                            radius={6}
                          />
                        ) : (
                          <span {...stylex.props(styles.noMark)} />
                        )}
                      </span>
                    }
                    endContent={
                      <span {...stylex.props(styles.end)}>
                        <span {...stylex.props(styles.state)} style={{ color: look.color }}>
                          <span {...stylex.props(styles.stateDot)} style={{ background: look.color }} />
                          {look.text}
                        </span>
                        <span {...stylex.props(styles.age)}>{ago(task.updated_at_ms)}</span>
                      </span>
                    }
                  />
                );
              })}
            </List>
          </div>
        </section>
        <aside aria-label="Task preview" {...stylex.props(styles.aside)}>
          {current ? (
            <Preview key={current.task.id} task={current.task} onOpen={() => openRun(current.task.id)} />
          ) : null}
        </aside>
      </div>
    </Page>
  );
}
