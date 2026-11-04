import type { ProjectSummary, TaskView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useChannels,
  useCompany,
  useEffectiveSets,
  useEmployees,
  useGuidelines,
  useIdeas,
  useOpenTasks,
  useProjects,
} from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Dialog, Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { BoardScreen } from "./board";
import { canPin, pinnedFirst, togglePin } from "./employee-logic";
import { pinKey, usePins } from "./employees";
import { IdeasBody } from "./ideas";
import { splitIdeas } from "./knowledge-logic";
import { AREA, Confirm, Empty, EYEBROW, FIELD, failureOf, PageHead, PILL, SMALL } from "./org-kit";
import { overlays } from "./overlays-store";
import { useFolderChooser } from "./run-tools";
import { ScheduleDialog } from "./settings-advanced";
import { TasksScreen } from "./tasks";
import { taskStateLine } from "./tasks-logic";
import { VaultBody } from "./vault";

const COUNT_WORDS = ["", "One", "Two", "Three"];
const people = (n: number, working: number) =>
  n === 0
    ? "Nobody yet"
    : `${COUNT_WORDS[n] ?? n} employee${n === 1 ? "" : "s"}${working ? ` · ${working} working` : ""}`;
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const isActive = (t: TaskView) => !["succeeded", "failed", "cancelled"].includes(t.status) || Boolean(t.live_run);

export function useProjectPins() {
  const { hostName } = useLinkState();
  return usePins(pinKey(hostName, "pinned_projects"));
}

function NewProject({ onClose }: { onClose: () => void }) {
  useKeyContext("AureliusDialog", true);
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const folders = useFolderChooser();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [folder, setFolder] = useState("");
  useAction("ChooseDialogFolder", () => folders.choose("Choose a project folder", setFolder));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const save = async () => {
    if (busy) return;
    if (name.trim() === "") return setError("Enter a name.");
    if (folder.trim() === "") return setError("Choose a folder for the project first (Ctrl+O).");
    setBusy(true);
    try {
      const receipt = await connection.write(
        "project.create",
        {
          name: name.trim(),
          folder: folder.trim(),
          ...(description.trim() ? { description: description.trim() } : {}),
        },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["employees"] });
      await client.invalidateQueries({ queryKey: ["projects"] });
      toast("Project created and saved locally.");
      onClose();
      const made = receipt.result;
      if (made.kind === "command" && made.value.type === "project_created") {
        void navigate({ to: "/projects/$id", params: { id: made.value.project_id } });
      }
    } catch (e) {
      setError(failureOf(e));
      setBusy(false);
    }
  };
  useAction("ConfirmPrimary", () => void save());
  return (
    <Dialog open onClose={onClose} title="New project" width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">New project</p>
        <p className="m-0 text-[12px] text-text3">
          A project works in a folder on the server. It needs no employee or repository. Choose… can also make a new
          folder.
        </p>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Name
          <input
            data-autofocus
            placeholder="Name, for example Atlas"
            className={FIELD}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Description
          <input
            placeholder="Short description (optional)"
            className={FIELD}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Project folder (required)
          <span className="flex gap-2">
            <input className={`${FIELD} font-mono`} value={folder} onChange={(e) => setFolder(e.target.value)} />
            <Button onClick={() => folders.choose("Choose a project folder", setFolder)}>Choose…</Button>
            {folders.dialog}
          </span>
        </label>
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy} onClick={() => void save()}>
          Create project
        </Button>
      </div>
    </Dialog>
  );
}

function ProjectSettings({ project, onClose }: { project: ProjectSummary; onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const platform = usePlatform();
  const folders = useFolderChooser();
  const { features } = useLinkState();
  const employees = useEmployees();
  const company = useCompany();
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description ?? "");
  const [problem, setProblem] = useState<string | null>(null);
  const refresh = () =>
    client
      .invalidateQueries({ queryKey: ["employees"] })
      .then(() => client.invalidateQueries({ queryKey: ["projects"] }));
  const run = async (write: () => Promise<unknown>, done?: string) => {
    setProblem(null);
    try {
      await write();
      await refresh();
      if (done) toast(done);
    } catch (e) {
      setProblem(failureOf(e));
    }
  };
  const canDescribe = features.includes("project_description");
  const tooLong = description.length > 2000;
  const dispatcher = project.dispatcher_employee_id ?? "";
  const defaultId = company.data?.company.dispatcher_employee_id;
  const defaultName = employees.data?.find((e) => e.id === defaultId)?.name ?? "none";
  const chosen = employees.data?.find((e) => e.id === (dispatcher || defaultId));
  const warning = !(dispatcher || defaultId)
    ? "◆ No dispatcher. Started tickets wait in Todo until you choose an owner."
    : chosen && chosen.status !== "active"
      ? `◆ ${chosen.name} is ${chosen.status}. Started tickets wait for you to choose an owner.`
      : null;
  return (
    <Dialog open onClose={onClose} title={`Edit project · ${project.name}`} width={720} top>
      <div className="flex max-h-[72vh] flex-col gap-4 overflow-y-auto p-[18px]">
        <p className="m-0 text-[15px] font-semibold">Edit project · {project.name}</p>
        <p className={EYEBROW}>GENERAL</p>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Name
          <span className="flex gap-2">
            <input
              data-autofocus
              placeholder="Project name"
              className={FIELD}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <Button
              disabled={name.trim() === "" || name.trim() === project.name}
              onClick={() =>
                void run(() =>
                  connection.write(
                    "project.rename",
                    { project_id: project.id, name: name.trim() },
                    { retry: false, expectedVersion: project.version },
                  ),
                )
              }
            >
              Save
            </Button>
          </span>
        </label>
        <div className="flex flex-col gap-1 text-[12px] text-text3">
          Description
          {canDescribe ? (
            <>
              <textarea
                rows={3}
                aria-label="Description"
                placeholder="What this project is about"
                className={AREA}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
              <span className={tooLong ? "text-red" : ""}>
                {tooLong
                  ? `Too long: ${description.length} of at most 2000 characters.`
                  : `${description.length} / 2000`}
              </span>
              <Button
                className="self-start"
                disabled={tooLong || description.trim() === (project.description ?? "")}
                onClick={() =>
                  void run(() =>
                    connection.write(
                      "project.set_description",
                      { project_id: project.id, description: description.trim() },
                      { retry: false, expectedVersion: project.version },
                    ),
                  )
                }
              >
                Save
              </Button>
            </>
          ) : (
            <span>This server is too old to edit the description; update it to change it here.</span>
          )}
        </div>
        <div className="flex flex-col gap-1 text-[12px] text-text3">
          Folder
          <code className="font-mono text-text2">{project.folder ?? "No folder"}</code>
          <span className="flex gap-2">
            {platform.openFolder && project.folder ? (
              <Button onClick={() => void platform.openFolder?.(project.folder as string)}>Open folder</Button>
            ) : null}
            <Button
              onClick={() =>
                folders.choose("Choose another folder", (path) => {
                  void run(() =>
                    connection.write("project.set_folder", { project_id: project.id, path }, { retry: false }),
                  );
                })
              }
            >
              Choose another folder…
            </Button>
            {folders.dialog}
          </span>
        </div>
        <p className={EYEBROW}>BOARD</p>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Dispatcher
          <select
            className={FIELD}
            value={dispatcher}
            onChange={(e) => {
              const id = e.target.value;
              const who = employees.data?.find((x) => x.id === id)?.name;
              void run(
                () =>
                  connection.write(
                    "project.update",
                    { project_id: project.id, dispatcher: { employee_id: id || null } },
                    { retry: false },
                  ),
                id
                  ? `${who} chooses the owners of this project's tickets.`
                  : "This project uses the company's dispatcher again.",
              );
            }}
          >
            <option value="">Company default ({defaultName})</option>
            {(employees.data ?? [])
              .filter((e) => e.status === "active" || e.id === dispatcher)
              .map((e) => (
                <option key={e.id} value={e.id}>
                  {e.name}
                </option>
              ))}
          </select>
          <span>Chooses an owner when a ticket of this project starts without one.</span>
          {warning ? <span className="text-amber">{warning}</span> : null}
        </label>
        {problem ? (
          <p role="alert" className="m-0 text-red">
            {problem}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Close</Button>
      </div>
    </Dialog>
  );
}

function useProjectActions(project: ProjectSummary | null, onEdit: () => void, onMemory: () => void) {
  const connection = useConnection();
  const client = useQueryClient();
  const platform = usePlatform();
  const navigate = useNavigate();
  const [scheduling, setScheduling] = useState(false);
  const [archiving, setArchiving] = useState(false);
  const refresh = () =>
    client
      .invalidateQueries({ queryKey: ["employees"] })
      .then(() => client.invalidateQueries({ queryKey: ["projects"] }));
  const options = project
    ? [
        ...(platform.openProjectWindow && !platform.windowProject
          ? [{ key: "window", label: "Open in new window" }]
          : []),
        { key: "edit", label: "Edit…" },
        ...(platform.openFolder && project.folder ? [{ key: "folder", label: "Open folder" }] : []),
        { key: "schedule", label: "New recurring task…" },
        { key: "export", label: "Export (Markdown + JSON)" },
        { key: "memory", label: "Memory" },
        { key: "pause", label: project.status === "paused" ? "Resume project" : "Pause project" },
        { key: "archive", label: "Archive project…", danger: true },
      ]
    : [];
  const setStatus = async (status: "active" | "paused" | "archived", done: string) => {
    if (!project) return;
    try {
      await connection.write("project.set_status", { project_id: project.id, status }, { retry: false });
      await refresh();
      toast(done);
    } catch (e) {
      toast(failureOf(e));
    }
  };
  const pick = (key: string) => {
    if (!project) return;
    if (key === "edit") onEdit();
    else if (key === "memory") onMemory();
    else if (key === "folder" && project.folder) void platform.openFolder?.(project.folder);
    else if (key === "pause")
      void (project.status === "paused"
        ? setStatus("active", "Project resumed.")
        : setStatus("paused", "Project paused."));
    else if (key === "archive") setArchiving(true);
    else if (key === "window" && project) void platform.openProjectWindow?.(project.id, project.name);
    else if (key === "schedule") setScheduling(true);
    else if (key === "export")
      void connection
        .write("project.export", { project_id: project.id }, { retry: false })
        .then((receipt) => {
          const made = receipt.result;
          const path = made.kind === "host" && made.value.type === "exported" ? made.value.markdown_path : "";
          toast(`Exported (Markdown + JSON) to ${path.replace(/[/\\][^/\\]*$/, "")}`);
        })
        .catch((e: unknown) => toast(failureOf(e)));
  };
  const confirm =
    archiving && project ? (
      <Confirm
        danger
        title={`Archive ${project.name}?`}
        body="The project disappears from Projects. Nothing is deleted; the history is kept."
        action="Archive"
        onClose={() => setArchiving(false)}
        onConfirm={() => {
          setArchiving(false);
          void setStatus("archived", "Project archived.").then(() => navigate({ to: "/projects" }));
        }}
      />
    ) : null;
  const dialogs = (
    <>
      {confirm}
      {scheduling && project ? (
        <ScheduleDialog schedule={null} projectId={project.id} onClose={() => setScheduling(false)} />
      ) : null}
    </>
  );
  return { options, pick, confirm: dialogs };
}

const GRID = "grid grid-cols-[minmax(240px,1.6fr)_180px_120px_110px_110px_36px] items-center gap-3 px-[18px]";

function ProjectRow({
  project,
  tasks,
  waiting,
  selected,
  pinned,
  onPin,
  onEdit,
}: {
  project: ProjectSummary;
  tasks: TaskView[];
  waiting: { approvals: number; decisions: number };
  selected: boolean;
  pinned: boolean;
  onPin: () => void;
  onEdit: () => void;
}) {
  const navigate = useNavigate();
  const employees = useEmployees();
  const team = (employees.data ?? []).filter((e) => e.projects_active.includes(project.id) && e.status !== "archived");
  const working = team.filter((e) => e.working).length;
  const active = tasks.filter(isActive).length;
  const need = waiting.approvals + waiting.decisions;
  const status =
    project.status === "paused"
      ? { label: "Paused", color: "text-text3" }
      : need > 0
        ? { label: `Needs you · ${need}`, color: "text-amber" }
        : active > 0
          ? { label: "Working", color: "text-acc-text" }
          : { label: "Quiet", color: "text-text3" };
  const wait =
    need === 0
      ? "Nothing waiting"
      : [
          waiting.approvals ? plural(waiting.approvals, "approval") : null,
          waiting.decisions ? plural(waiting.decisions, "decision") : null,
        ]
          .filter(Boolean)
          .join(", ");
  const { options, pick, confirm } = useProjectActions(
    project,
    onEdit,
    () => void navigate({ to: "/knowledge/$tab", params: { tab: "memory" }, search: { project: project.id } }),
  );
  return (
    <div
      className={`${GRID} group min-h-[60px] border-b border-line2 hover:bg-sel ${selected ? "bg-sel shadow-[inset_2px_0_0_var(--acc-text)]" : ""}`}
    >
      <span className="flex min-w-0 items-center gap-3">
        <Mark letter={initial(project.name)} color={markColor(project.id)} size={28} radius={7} />
        <span className="min-w-0">
          <Link
            to="/projects/$id"
            params={{ id: project.id }}
            className="block truncate font-semibold text-text no-underline hover:underline"
          >
            {project.name}
          </Link>
          <span className="block truncate text-[12px] text-text3">{project.description || "No description"}</span>
        </span>
      </span>
      <span className="min-w-0">
        <span className="flex">
          {team.slice(0, 4).map((e) => (
            <Mark key={e.id} letter={initial(e.name)} color={markColor(e.id)} size={16} radius={4} />
          ))}
          {team.length > 4 ? <span className="ml-1 text-[11px] text-text3">+{team.length - 4}</span> : null}
        </span>
        <span className="block truncate text-[12px] text-text3">{people(team.length, working)}</span>
      </span>
      <span className="text-[12px] text-text2">{wait}</span>
      <span className="text-[12px] text-text2">{plural(active, "active task")}</span>
      <span className={`text-[12px] ${status.color}`}>{status.label}</span>
      <span className="flex items-center gap-1">
        <button
          type="button"
          aria-label={`${pinned ? "Unpin" : "Pin"} ${project.name}`}
          aria-pressed={pinned}
          title={pinned ? "Unpin from the sidebar" : "Pin to the sidebar"}
          onClick={onPin}
          className={`cursor-pointer border-0 bg-transparent p-0 text-[12px] focus-visible:opacity-100 group-hover:opacity-100 ${pinned ? "text-acc-text" : "text-text3 opacity-0"}`}
        >
          {pinned ? "★" : "☆"}
        </button>
        <Menu
          label={`More for ${project.name}`}
          options={options}
          onPick={pick}
          trigger={
            <button
              type="button"
              aria-label={`More for ${project.name}`}
              className="cursor-pointer border-0 bg-transparent p-0 text-text3 hover:text-text"
            >
              ⋯
            </button>
          }
        />
      </span>
      {confirm}
    </div>
  );
}

export function ProjectsScreen() {
  const navigate = useNavigate();
  const projects = useProjects();
  const tasks = useOpenTasks();
  const [pins, setPins] = useProjectPins();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<ProjectSummary | null>(null);
  const [selected, setSelected] = useState(0);
  const list = useMemo(
    () =>
      pinnedFirst(
        (projects.data ?? []).filter((p) => p.status !== "archived"),
        pins,
      ),
    [projects.data, pins],
  );
  const forProject = (id: string) => (tasks.data ?? []).filter((t) => t.project_id === id);
  const waitingOf = (id: string) => {
    const mine = forProject(id);
    return {
      approvals: mine.reduce((n, t) => n + t.pending_approvals, 0),
      decisions: mine.filter((t) => t.status === "waiting" && t.pending_approvals === 0 && t.waiting_reason === "input")
        .length,
    };
  };
  useAction("NewProject", () => setAdding(true));
  useAction("SelectNext", () => setSelected((s) => Math.min(list.length - 1, s + 1)));
  useAction("SelectPrev", () => setSelected((s) => Math.max(0, s - 1)));
  useAction("OpenSelected", () => {
    const p = list[selected];
    if (p) void navigate({ to: "/projects/$id", params: { id: p.id } });
  });
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead title="Projects">
        <Button kind="primary" className={SMALL} onClick={() => setAdding(true)}>
          New project
        </Button>
      </PageHead>
      <div className="min-h-0 flex-1 overflow-auto">
        {projects.data && list.length === 0 ? (
          <Empty title="Begin with what is in your control.">
            <p className="m-0 text-text2">Create a project to give an employee a mandate.</p>
            <Button kind="primary" onClick={() => setAdding(true)}>
              New project
            </Button>
          </Empty>
        ) : null}
        {list.length > 0 ? (
          <div className="min-w-[760px]">
            <div className={`${GRID} h-8 border-b border-line2 text-[12px] text-text3`}>
              <span>Project</span>
              <span>People</span>
              <span>Waiting</span>
              <span>Active</span>
              <span>Status</span>
              <span />
            </div>
            {list.map((p, i) => (
              <ProjectRow
                key={p.id}
                project={p}
                tasks={forProject(p.id)}
                waiting={waitingOf(p.id)}
                selected={i === selected}
                pinned={pins.includes(p.id)}
                onEdit={() => setEditing(p)}
                onPin={() => {
                  if (!canPin(pins, p.id)) return toast("Only 5 projects can be pinned. Unpin one first.");
                  setPins(togglePin(pins, p.id));
                }}
              />
            ))}
          </div>
        ) : null}
      </div>
      {adding ? <NewProject onClose={() => setAdding(false)} /> : null}
      {editing ? <ProjectSettings project={editing} onClose={() => setEditing(null)} /> : null}
    </div>
  );
}

type ProjectTab = "overview" | "board" | "tasks" | "ideas" | "vault";
const TABS: { key: ProjectTab; label: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "board", label: "Board" },
  { key: "tasks", label: "Tasks" },
  { key: "ideas", label: "Ideas" },
  { key: "vault", label: "Vault" },
];

function Card({ title, right, children }: { title: string; right?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="flex min-w-0 flex-col overflow-hidden rounded-[10px] border border-line bg-panel">
      <header className="flex h-10 flex-none items-center gap-2 border-b border-line2 px-[14px]">
        <p className={`${EYEBROW} min-w-0 truncate`} title={title}>
          {title}
        </p>
        <span className="ml-auto flex flex-none items-center gap-2 text-[12px] whitespace-nowrap">{right}</span>
      </header>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

function Overview({ project }: { project: ProjectSummary }) {
  const connection = useConnection();
  const client = useQueryClient();
  const employees = useEmployees();
  const tasks = useOpenTasks();
  const ideas = useIdeas(project.id);
  const channels = useChannels();
  const guidelines = useGuidelines();
  const team = (employees.data ?? []).filter((e) => e.projects_active.includes(project.id) && e.status !== "archived");
  const open = (tasks.data ?? []).filter((t) => t.project_id === project.id && isActive(t));
  const openIdeas = (ideas.data ?? []).filter((i) => i.status === "open");
  const split = splitIdeas(openIdeas);
  const linked = (guidelines.data ?? []).filter((g) =>
    g.links.some((l) => l.kind === "project" && l.id === project.id),
  );
  const pairs = useMemo(() => team.map((e): [string, string] => [e.id, project.id]), [team, project.id]);
  const sets = useEffectiveSets(pairs);
  const sizes = [...(sets.data?.values() ?? [])];
  const largest = sizes.sort((a, b) => b.total_chars - a.total_chars)[0];
  const largestName = team.find((e) => e.id === largest?.employee_id)?.name;
  const added = linked.reduce((n, g) => n + g.chars, 0);
  const channel = channels.data?.find((c) => c.project_id === project.id && !c.ticket_id);
  const row =
    "flex min-h-[42px] items-center gap-[10px] border-b border-line2 px-[14px] text-text no-underline last:border-b-0 hover:bg-sel";
  return (
    <div className="grid min-h-0 flex-1 auto-rows-min grid-cols-[repeat(auto-fit,minmax(340px,1fr))] gap-4 overflow-y-auto p-[22px]">
      {project.status === "paused" ? (
        <div className="col-span-full flex items-center gap-3 rounded-[8px] border border-amber-line bg-amber-soft px-3 py-[10px]">
          <span className="flex-1">This project is paused. No new automatic work starts.</span>
          <Button
            className={SMALL}
            onClick={() =>
              void connection
                .write("project.set_status", { project_id: project.id, status: "active" }, { retry: false })
                .then(() => client.invalidateQueries({ queryKey: ["employees"] }))
                .then(() => toast("Project resumed."))
            }
          >
            Resume
          </Button>
        </div>
      ) : null}
      <Card
        title={`WHO WORKS HERE · ${team.length}`}
        right={
          channel ? (
            <Link to="/chat" search={{ channel: channel.conversation_id }} className="text-acc-text no-underline">
              Open #{channel.name}
            </Link>
          ) : null
        }
      >
        {team.length === 0 ? (
          <p className="m-0 p-[14px] text-text3">Nobody works here yet. Start a ticket to put someone to work.</p>
        ) : null}
        {team.map((e) => (
          <Link key={e.id} to="/employees/$id" params={{ id: e.id }} className={row}>
            <Mark letter={initial(e.name)} color={markColor(e.id)} size={22} radius={6} />
            <span className="min-w-0 flex-1 truncate font-medium">{e.name}</span>
            <span className={`text-[12px] ${e.working ? "text-acc-text" : "text-text3"}`}>
              {e.status === "paused" ? "Paused" : e.working ? "Working" : "Available"}
            </span>
          </Link>
        ))}
      </Card>
      <Card
        title={`TASKS · ${open.length} OPEN`}
        right={
          <Link
            to="/projects/$id"
            params={{ id: project.id }}
            search={{ tab: "tasks" }}
            className="text-acc-text no-underline"
          >
            All →
          </Link>
        }
      >
        {open.length === 0 ? <p className="m-0 p-[14px] text-text3">No open tasks.</p> : null}
        {open.slice(0, 6).map((t) => {
          const line = taskStateLine(t);
          return (
            <Link key={t.id} to="/tasks/$taskId" params={{ taskId: t.id }} className={row}>
              <span className="min-w-0 flex-1 truncate">
                {t.ticket_number ? (
                  <span className="mr-2 font-mono text-[12px] text-text3">#{t.ticket_number}</span>
                ) : null}
                {t.title}
              </span>
              <span className="flex-none text-[12px]" style={{ color: line.color }}>
                {line.text}
              </span>
            </Link>
          );
        })}
        <div className="p-[10px]">
          <Button
            className={SMALL}
            onClick={() => overlays.open("ticket", { title: "", description: "", projectId: project.id })}
          >
            New ticket
          </Button>
        </div>
      </Card>
      <Card
        title={`IDEAS · ${openIdeas.length} OPEN`}
        right={
          <span className="text-text3">
            FROM YOU · {split.yours.length} · FROM EMPLOYEES · {split.employees.length}
          </span>
        }
      >
        {ideas.isPending ? <p className="m-0 p-[14px] text-text3">Loading ideas…</p> : null}
        {ideas.data && openIdeas.length === 0 ? (
          <p className="m-0 p-[14px] text-text3">No open ideas. Note one in the Ideas tab with Ctrl+N.</p>
        ) : null}
        {openIdeas.slice(0, 5).map((i) => (
          <Link key={i.id} to="/projects/$id" params={{ id: project.id }} search={{ tab: "ideas" }} className={row}>
            <span className="min-w-0 flex-1 truncate">{i.title}</span>
            <span className="flex-none text-[12px] text-text3">{i.author}</span>
          </Link>
        ))}
      </Card>
      <Card
        title={`GUIDELINES FOR EVERYONE IN ${project.name.toUpperCase()}`}
        right={
          <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} className="text-acc-text no-underline">
            Manage links…
          </Link>
        }
      >
        {linked.length === 0 ? (
          <p className="m-0 p-[14px] text-text3">None: runs here get only their employee's guidelines.</p>
        ) : null}
        {linked.map((g) => (
          <Link key={g.id} to="/knowledge/guidelines/$id" params={{ id: g.id }} className={row}>
            <span className="min-w-0 flex-1 truncate">{g.title}</span>
            <span className="font-mono text-[12px] text-text3">{g.chars}</span>
          </Link>
        ))}
        {linked.length > 0 ? (
          <p className="m-0 border-t border-line2 p-[14px] text-[12px] text-text3">
            Added to each run here: {added} characters
            {largest && largestName ? ` · largest block: ${largestName} ${largest.total_chars}` : ""}
          </p>
        ) : null}
      </Card>
    </div>
  );
}

export function ProjectPage({ id, tab }: { id: string; tab: ProjectTab }) {
  const navigate = useNavigate();
  const projects = useProjects();
  const project = projects.data?.find((p) => p.id === id) ?? null;
  const [editing, setEditing] = useState(false);
  const { options, pick, confirm } = useProjectActions(
    project,
    () => setEditing(true),
    () =>
      void navigate({
        to: "/knowledge/$tab",
        params: { tab: "memory" },
        search: project ? { project: project.id } : {},
      }),
  );
  const go = (delta: number) => {
    const at = TABS.findIndex((t) => t.key === tab);
    const next = TABS[(at + delta + TABS.length) % TABS.length];
    if (next) void navigate({ to: "/projects/$id", params: { id }, search: { tab: next.key } });
  };
  useAction("NextTab", () => go(1));
  useAction("PrevTab", () => go(-1));
  useAction("Back", () => void navigate({ to: "/projects" }));
  useAction("OpenProjectSettings", () => setEditing(true), project !== null);
  useAction("EditItem", () => setEditing(true), project !== null);
  useAction("ChangeProjectFolder", () => setEditing(true), project !== null);
  useAction("ExportProject", () => pick("export"), project !== null);
  useAction("TogglePause", () => pick("pause"), project !== null);
  useAction("ArchiveItem", () => pick("archive"), project !== null);
  useAction("OpenMemory", () => pick("memory"), project !== null);
  useAction("OpenProjectFolder", () => pick("folder"), project !== null);
  useEffect(() => {
    if (projects.data && project?.status === "archived") void navigate({ to: "/projects" });
  }, [projects.data, project, navigate]);
  if (!project) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <PageHead
          title={
            <Link to="/projects" className="text-text2 no-underline">
              Back to projects
            </Link>
          }
        />
        {projects.data ? <p className="m-0 p-6 text-text2">This project no longer exists.</p> : null}
      </div>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead
        title={
          <>
            <Link to="/projects" aria-label="Back to projects" className="text-text3 no-underline hover:text-text">
              ‹
            </Link>
            <Mark letter={initial(project.name)} color={markColor(project.id)} size={16} radius={4} />
            <span className="truncate font-semibold">{project.name}</span>
          </>
        }
        tabs={
          <nav aria-label="Project" className="flex gap-px rounded-[6px] border border-line2 bg-sunk p-[2px]">
            {TABS.map((t) => (
              <Link
                key={t.key}
                to="/projects/$id"
                params={{ id }}
                search={{ tab: t.key }}
                aria-current={tab === t.key ? "page" : undefined}
                className={`${PILL} ${tab === t.key ? "bg-sel !text-text" : ""}`}
              >
                {t.label}
              </Link>
            ))}
          </nav>
        }
      >
        {project.folder ? (
          <code title={project.folder} className="max-w-[260px] truncate font-mono text-[12px] text-text3">
            {project.folder}
          </code>
        ) : null}
        <Button className={SMALL} onClick={() => setEditing(true)}>
          Edit project
        </Button>
        <Menu
          label={`More for ${project.name}`}
          options={options}
          onPick={pick}
          trigger={
            <button
              type="button"
              aria-label={`More for ${project.name}`}
              className="h-7 cursor-pointer rounded-[6px] border border-btn-line bg-btn px-2 text-text2"
            >
              ⋯
            </button>
          }
        />
      </PageHead>
      {tab === "overview" ? <Overview project={project} /> : null}
      {tab === "board" ? <BoardScreen ticket={undefined} projectId={project.id} /> : null}
      {tab === "tasks" ? <TasksScreen projectId={project.id} /> : null}
      {tab === "ideas" ? <IdeasBody projectId={project.id} chrome={false} /> : null}
      {tab === "vault" ? (
        <div className="flex min-h-0 flex-1">
          <VaultBody projectId={project.id} scopeName={project.name} />
        </div>
      ) : null}
      {editing ? <ProjectSettings project={project} onClose={() => setEditing(false)} /> : null}
      {confirm}
    </div>
  );
}
