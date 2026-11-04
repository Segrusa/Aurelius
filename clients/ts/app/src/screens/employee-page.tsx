import type { EmployeeSummary, TaskView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useEffectiveSets, useEmployees } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { EmployeeDialog } from "./employee-dialog";
import { type AgentMode, canPin, MODE_LABELS, nextMode, position, threadGroup, togglePin } from "./employee-logic";
import { employeeState, pinKey, usePins } from "./employees";
import { Confirm, EYEBROW, failureOf, PageHead, SMALL, shortDay } from "./org-kit";
import { overlays } from "./overlays-store";
import { taskStateLine } from "./tasks-logic";

const CHIP =
  "inline-flex h-6 cursor-pointer items-center gap-[6px] rounded-[6px] border border-line bg-transparent px-2 text-[12px] text-text2 hover:text-text";

type Ask = "browser" | "screen" | "archive" | null;

const isOngoing = (t: TaskView) => !["succeeded", "failed", "cancelled"].includes(t.status) || Boolean(t.live_run);

function TaskCard({ task }: { task: TaskView }) {
  const line = taskStateLine(task);
  return (
    <Link
      to="/tasks/$taskId"
      params={{ taskId: task.id }}
      className="flex flex-col gap-[3px] rounded-[8px] border border-line bg-panel px-3 py-[10px] text-text no-underline hover:bg-sel"
    >
      <span className="truncate font-medium">{task.title}</span>
      <span className="truncate text-[12px] text-text3">
        {task.ticket_number ? `#${task.ticket_number} · ` : ""}
        <span style={{ color: line.color }}>{line.text}</span> · {shortDay(task.updated_at_ms)}
      </span>
    </Link>
  );
}

export function EmployeePage({ id }: { id: string }) {
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const platform = usePlatform();
  const { hostName } = useLinkState();
  const employees = useEmployees();
  const tasks = useAllTasks();
  const employee = employees.data?.find((e) => e.id === id && e.status !== "archived") ?? null;
  const [editing, setEditing] = useState(false);
  const [ask, setAsk] = useState<Ask>(null);
  const [showEarlier, setShowEarlier] = useState(false);
  const [pins, setPins] = usePins(pinKey(hostName, "pinned_employees"));
  const pairs = useMemo((): [string, null][] => [[id, null]], [id]);
  const sets = useEffectiveSets(pairs);
  const set = sets.data?.get(`${id}|`);
  const mine = useMemo(
    () =>
      (tasks.data ?? [])
        .filter((t) => t.employee_id === id && t.conversation_id)
        .sort((a, b) => Number(isOngoing(b)) - Number(isOngoing(a)) || b.updated_at_ms - a.updated_at_ms),
    [tasks.data, id],
  );
  const now = Date.now();
  const byGroup = (g: string) => mine.filter((t) => threadGroup(isOngoing(t), false, t.updated_at_ms, now) === g);
  const refresh = () => client.invalidateQueries({ queryKey: ["employees"] });
  const act = async (run: () => Promise<unknown>, done?: string) => {
    try {
      await run();
      await refresh();
      if (done) toast(done);
    } catch (e) {
      toast(failureOf(e));
    }
  };
  useAction("EditItem", () => setEditing(true), employee !== null);
  useAction("Back", () => void navigate({ to: "/employees" }));

  if (employees.data && !employee) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <PageHead
          title={
            <Link to="/employees" className="text-text2 no-underline">
              Employees
            </Link>
          }
        />
        <p className="m-0 p-6 text-text2">This employee no longer exists (archived?). Esc goes back.</p>
      </div>
    );
  }
  if (!employee) return <p className="m-0 p-6 text-text3">Loading…</p>;
  const e: EmployeeSummary = employee;
  const state = employeeState(e);
  const boss = employees.data?.find((b) => b.id === e.reports_to);
  const pinned = pins.includes(e.id);
  const setMode = () => {
    if (!e.conversation_id) return;
    const mode: AgentMode = nextMode(e.agent_mode);
    void act(() =>
      connection.write(
        "conversation.set_agent_mode",
        { conversation_id: e.conversation_id as string, mode },
        { retry: false },
      ),
    );
  };
  const setAllowed = (
    method: "conversation.set_browser_allowed" | "conversation.set_screen_allowed",
    allowed: boolean,
  ) =>
    e.conversation_id
      ? act(() => connection.write(method, { conversation_id: e.conversation_id as string, allowed }, { retry: false }))
      : Promise.resolve();
  const browser = () =>
    e.browser_allowed ? void setAllowed("conversation.set_browser_allowed", false) : setAsk("browser");
  const screen = () =>
    e.screen_allowed ? void setAllowed("conversation.set_screen_allowed", false) : setAsk("screen");
  const menu = [
    { key: "edit", label: "Edit employee…" },
    { key: "pin", label: pinned ? "Unpin from the list" : "Pin to the list" },
    { key: "mode", label: "Switch mode" },
    { key: "browser", label: e.browser_allowed ? "Browser off" : "Browser on" },
    { key: "screen", label: e.screen_allowed ? "Screen off" : "Screen on" },
    ...(platform.pickFolder ? [{ key: "folder", label: "Choose workspace folder…" }] : []),
    ...(platform.openFolder && e.folder ? [{ key: "open", label: "Open workspace folder" }] : []),
    { key: "pause", label: e.status === "paused" ? "Resume employee" : "Pause employee" },
    { key: "archive", label: "Archive employee…", danger: true },
  ];
  const pick = (key: string) => {
    if (key === "edit") setEditing(true);
    else if (key === "pin") {
      if (!canPin(pins, e.id)) toast("Only 5 employees can be pinned. Unpin one first.");
      else setPins(togglePin(pins, e.id));
    } else if (key === "mode") setMode();
    else if (key === "browser") browser();
    else if (key === "screen") screen();
    else if (key === "folder")
      void platform.pickFolder?.(`Choose a folder for ${e.name}`).then((path) => {
        if (path)
          void act(() => connection.write("employee.set_folder", { employee_id: e.id, path }, { retry: false }));
      });
    else if (key === "open" && e.folder) void platform.openFolder?.(e.folder);
    else if (key === "pause") {
      const paused = e.status === "paused";
      void act(
        () =>
          connection.write(
            "employee.set_status",
            { employee_id: e.id, status: paused ? "active" : "paused" },
            { retry: false },
          ),
        paused ? "Employee resumed." : "Employee paused: no new work starts.",
      );
    } else if (key === "archive") setAsk("archive");
  };
  const give = () => overlays.open("give", { title: "", description: "", employeeId: e.id });
  const past = byGroup("past");
  const earlier = byGroup("earlier");
  const active = byGroup("active");
  const chipOn = (on: boolean) => (on ? "!text-acc-text" : "");
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead
        title={
          <>
            <Link to="/employees" className="text-text2 no-underline hover:text-text">
              Employees
            </Link>
            <span className="text-text3">›</span>
            <span className="truncate">{e.name}</span>
          </>
        }
      >
        <Link to="/chat" className="text-[12px] text-text2 no-underline hover:text-text">
          Talk in Team chat
        </Link>
        <Button className={SMALL} onClick={() => setEditing(true)}>
          Edit
        </Button>
        <Menu
          label={`More for ${e.name}`}
          options={menu}
          onPick={pick}
          trigger={
            <button
              type="button"
              aria-label={`More for ${e.name}`}
              className="h-7 cursor-pointer rounded-[6px] border border-btn-line bg-btn px-2 text-text2"
            >
              ⋯
            </button>
          }
        />
        <Button kind="primary" className={SMALL} onClick={give}>
          Give work
        </Button>
      </PageHead>
      <div className="flex min-h-0 flex-1 flex-wrap overflow-y-auto">
        <div className="flex min-w-0 flex-[999_1_480px] flex-col gap-4 px-8 pt-[22px] pb-8">
          <div className="flex items-center gap-[14px]">
            <Mark letter={initial(e.name)} color={markColor(e.id)} size={44} radius={11} />
            <div className="min-w-0">
              <h1 className="m-0 truncate text-[20px] font-semibold tracking-[-0.01em]">{e.name}</h1>
              <p className="m-0 truncate text-text2">
                {position(e)}
                {boss ? ` · reports to ${boss.name}` : ""}
              </p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <span
              className={`inline-flex h-6 items-center gap-[6px] rounded-[6px] border border-line px-2 text-[12px] ${state.color}`}
            >
              <span className={`size-[6px] rounded-full ${state.dot}`} />
              {state.label}
            </span>
            <button type="button" title="Its mode for new work" className={CHIP} onClick={setMode}>
              Mode: {MODE_LABELS[e.agent_mode]}
            </button>
            <button type="button" className={`${CHIP} ${chipOn(e.browser_allowed)}`} onClick={browser}>
              Browser {e.browser_allowed ? "on" : "off"}
            </button>
            <button type="button" className={`${CHIP} ${chipOn(e.screen_allowed)}`} onClick={screen}>
              Screen: {e.screen_allowed ? "allowed" : "asks"}
            </button>
          </div>
          {e.instructions ? (
            <section className="flex flex-col gap-2">
              <p className={EYEBROW}>INSTRUCTIONS</p>
              <p className="m-0 line-clamp-[12] leading-[1.6] whitespace-pre-wrap text-text2">{e.instructions}</p>
            </section>
          ) : null}
          {mine.length === 0 ? (
            <div className="flex flex-col gap-1 text-text2">
              <p className="m-0 font-medium">{e.name} has no tasks yet.</p>
              <p className="m-0 text-[12px] text-text3">
                Tasks you give {e.name} show on the right, each with its conversation. Talk with {e.name} in the Team
                chat.
              </p>
            </div>
          ) : null}
          <Link to="/chat" className="text-acc-text no-underline">
            Talk with {e.name} in Team chat →
          </Link>
        </div>
        <aside
          aria-label="Tasks"
          className="flex min-w-0 flex-[1_1_320px] flex-col gap-3 border-l border-line bg-sunk p-[18px]"
        >
          <p className="m-0 flex font-medium">
            Tasks{" "}
            <span className="ml-auto text-[12px] font-normal text-text3">
              {active.length} active · {mine.length}
            </span>
          </p>
          {tasks.data && mine.length === 0 ? (
            <p className="m-0 text-[12px] text-text3">Tasks you give this employee show here, from every project.</p>
          ) : null}
          {tasks.data && mine.length > 0 && active.length === 0 ? (
            <p className="m-0 text-[12px] text-text3">Nothing active</p>
          ) : null}
          {active.map((t) => (
            <TaskCard key={t.id} task={t} />
          ))}
          {past.length > 0 ? <p className={EYEBROW}>Past</p> : null}
          {past.map((t) => (
            <TaskCard key={t.id} task={t} />
          ))}
          {earlier.length > 0 ? (
            <button
              type="button"
              aria-expanded={showEarlier}
              onClick={() => setShowEarlier(!showEarlier)}
              className="cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-text3 hover:text-text"
            >
              {showEarlier ? "▾" : "▸"} Earlier · {earlier.length}
            </button>
          ) : null}
          {showEarlier ? earlier.map((t) => <TaskCard key={t.id} task={t} />) : null}
          <div className="mt-2 flex flex-col gap-1 border-t border-line2 pt-3">
            <p title="What its runs get on top of its instructions" className="m-0 font-medium">
              Guidelines{set ? ` · ${set.guidelines.length} · ${set.total_chars} / ${set.limit_chars}` : ""}
            </p>
            {set?.guidelines.map((g) => (
              <Link
                key={g.id}
                to="/knowledge/guidelines/$id"
                params={{ id: g.id }}
                className="truncate text-[12px] text-text2 no-underline hover:text-text"
              >
                {g.title}
              </Link>
            ))}
            <Link
              to="/knowledge/$tab"
              params={{ tab: "guidelines" }}
              className="text-[12px] text-acc-text no-underline"
            >
              Manage links →
            </Link>
          </div>
        </aside>
      </div>
      {editing ? <EmployeeDialog employee={e} onClose={() => setEditing(false)} /> : null}
      {ask === "browser" ? (
        <Confirm
          title="Allow the browser?"
          body="This agent can then use your own Chrome, which is signed in to your accounts. Every browser action is still shown to you first, also in Auto mode. Task workers never get the browser."
          action="Allow browser"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            void setAllowed("conversation.set_browser_allowed", true);
          }}
        />
      ) : null}
      {ask === "screen" ? (
        <Confirm
          title="Let this agent see your screen?"
          body="It can then take screenshots of your screen or of one window without asking, whenever it thinks that helps. A screenshot shows anything that is open, also private things. Off, it still asks you for each screenshot. Task workers never get this."
          action="Allow screen"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            void setAllowed("conversation.set_screen_allowed", true);
          }}
        />
      ) : null}
      {ask === "archive" ? (
        <Confirm
          danger
          title={`Archive ${e.name}?`}
          body="The employee stops starting work and disappears from the lists. Conversations and history are kept."
          action="Archive"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            void act(
              () =>
                connection.write("employee.set_status", { employee_id: e.id, status: "archived" }, { retry: false }),
              "Archived. The history is kept.",
            ).then(() => navigate({ to: "/employees" }));
          }}
        />
      ) : null}
    </div>
  );
}
