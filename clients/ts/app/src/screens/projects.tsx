import { Button } from "@astryxdesign/core/Button";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { TextInput } from "@astryxdesign/core/TextInput";
import type { ProjectSummary, RailPinsView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useOpenTasks, useProjects, useRailRead } from "../data";
import { type Feature, shown } from "../hidden";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { pinnedFirst, togglePin } from "./employee-logic";
import { pinKey, usePins, useStoredPins } from "./employees";
import { Confirm, Empty, FormDialog, failureOf, Hint, Page, PageHead, Problem } from "./org-kit";
import { useFolderChooser } from "./run-tools";
import { FindField, Grow, Lede, Segments, SettingsPage, Toolbar } from "./settings-frame";
import { Chip, focusField } from "./settings-kit";
import { agoText } from "./settings-logic";
import { VaultBody } from "./vault";
import { chosenRail, openProjects, RAIL_PINS, RAIL_PINS_FEATURE, railPins } from "./workspace-logic";
import { WorkspaceSettingsBody } from "./workspace-settings";

// Selector takes no empty value, so "the company's default dispatcher" needs a word of its own.
const _COMPANY_DEFAULT = "company-default";

const styles = stylex.create({
  withAction: { display: "flex", alignItems: "flex-end", gap: 8 },
  grow: { minWidth: 0, flex: 1 },
  mono: { fontFamily: "var(--font-family-code)" },
  scroll: { minHeight: 0, flex: 1, overflow: "auto" },
  table: { minWidth: 760 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  // A cell has max-width: 0 for truncating, which would close these columns.
  peopleColumn: { width: 180, minWidth: 180, maxWidth: 180 },
  waitingColumn: { width: 120, minWidth: 120, maxWidth: 120 },
  activeColumn: { width: 110, minWidth: 110, maxWidth: 110 },
  statusColumn: { width: 150, minWidth: 150, maxWidth: 150 },
  moreColumn: { width: 64, minWidth: 64, maxWidth: 64 },
  row: { height: 56, cursor: "pointer" },
  folder: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  card: {
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    backgroundColor: "var(--color-background-card)",
    boxShadow: "var(--precision-card-shadow)",
  },
  selected: { backgroundColor: "var(--color-overlay-hover)" },
  selectedEdge: { boxShadow: "inset 2px 0 0 var(--color-text-accent)" },
  projectCell: { display: "flex", minWidth: 0, alignItems: "center", gap: 12 },
  minZero: { minWidth: 0 },
  projectName: {
    display: "block",
    overflow: "hidden",
    fontWeight: 600,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-primary)",
    textDecoration: { default: "none", ":hover": "underline" },
  },
  line: {
    display: "block",
    overflow: "hidden",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  marks: { display: "flex" },
  moreMarks: { marginInlineStart: 4, fontSize: 11, color: "var(--precision-text3)" },
  small: { fontSize: 12, color: "var(--color-text-secondary)" },
  actions: { display: "flex", alignItems: "center", gap: 4 },
  pin: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: "var(--precision-text3)",
    opacity: { default: 0, ":focus-visible": 1, [stylex.when.ancestor(":hover")]: 1 },
  },
  pinned: { color: "var(--color-text-accent)", opacity: 1 },
  missing: { margin: 0, padding: 24, color: "var(--color-text-secondary)" },
  back: { color: "var(--color-text-secondary)", textDecoration: "none" },
  fill: { display: "flex", minHeight: 0, flex: 1 },
});

const COUNT_WORDS = ["", "One", "Two", "Three"];
const _people = (n: number, working: number) =>
  n === 0
    ? "Nobody yet"
    : `${COUNT_WORDS[n] ?? n} employee${n === 1 ? "" : "s"}${working ? ` · ${working} working` : ""}`;
const _plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const isActive = (t: TaskView) => !["succeeded", "failed", "cancelled"].includes(t.status) || Boolean(t.live_run);

/**
 * The workspaces the board member pinned to their rail. The server keeps them per board member, so they
 * follow them to every device; an older server cannot, and then they stay on this device. All workspaces
 * lists every workspace, pinned or not.
 */
export function useRailPins() {
  const { hostName, features } = useLinkState();
  const connection = useConnection();
  const client = useQueryClient();
  const projects = useProjects();
  const serverKeeps = features.includes(RAIL_PINS_FEATURE);
  const server = useRailRead(serverKeeps);
  const [device, setDevice] = useStoredPins(pinKey(hostName, RAIL_PINS));
  // The stars of before; they only order the rail until the board member pins for themselves.
  const [legacy] = usePins(pinKey(hostName, "pinned_projects"));
  const stored = chosenRail(serverKeeps, server.data, device);
  const pins = useMemo(
    () => railPins(stored ?? null, openProjects(projects.data ?? []), legacy),
    [stored, projects.data, legacy],
  );
  const set = (next: string[]) => {
    if (!serverKeeps) return setDevice(next);
    const before = server.data;
    // The rail moves at once; the server's answer and event confirm it.
    client.setQueryData<RailPinsView>(["rail"], { chosen: true, project_ids: next });
    connection.write("user.set_rail_pins", { project_ids: next }, { retry: false }).catch((e: unknown) => {
      client.setQueryData(["rail"], before);
      void client.invalidateQueries({ queryKey: ["rail"] });
      toast(`Could not change your rail: ${failureOf(e)}`);
    });
  };
  return {
    pins,
    /** Not before the projects and the rail are in, or the default "all of them" would be written as "none". */
    toggle: (id: string) => {
      if (projects.data && stored !== undefined) set(togglePin(pins, id));
    },
    /** A workspace you just made goes on your rail; under the default it is there already. */
    pin: (id: string) => {
      if (stored && !stored.includes(id)) set([...stored, id]);
    },
  };
}

function NewProject({ onClose }: { onClose: () => void }) {
  useKeyContext("AureliusDialog", true);
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const rail = useRailPins();
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
        rail.pin(made.value.project_id);
        void navigate({ to: "/projects/$id", params: { id: made.value.project_id } });
      }
    } catch (e) {
      setError(failureOf(e));
      setBusy(false);
    }
  };
  useAction("ConfirmPrimary", () => void save());
  return (
    <FormDialog
      title="New project"
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Create project" isDisabled={busy} onClick={() => void save()} />
        </>
      }
    >
      <Hint>
        A project works in a folder on the server. It needs no employee or repository. Choose… can also make a new
        folder.
      </Hint>
      <TextInput hasAutoFocus label="Name" placeholder="Name, for example Atlas" value={name} onChange={setName} />
      <TextInput
        label="Description"
        placeholder="Short description (optional)"
        value={description}
        onChange={setDescription}
      />
      <div {...stylex.props(styles.withAction)}>
        <span {...stylex.props(styles.grow)}>
          <TextInput
            width="100%"
            label="Project folder (required)"
            value={folder}
            onChange={setFolder}
            controlXstyle={styles.mono}
          />
        </span>
        <Button label="Choose…" onClick={() => folders.choose("Choose a project folder", setFolder)} />
      </div>
      {folders.dialog}
      {error ? <Problem>{error}</Problem> : null}
    </FormDialog>
  );
}

function useProjectActions(project: ProjectSummary | null, onEdit: () => void, onMemory: () => void) {
  const connection = useConnection();
  const client = useQueryClient();
  const platform = usePlatform();
  const navigate = useNavigate();
  const workspace = useWorkspace();
  const [archiving, setArchiving] = useState(false);
  const refresh = () =>
    client
      .invalidateQueries({ queryKey: ["employees"] })
      .then(() => client.invalidateQueries({ queryKey: ["projects"] }));
  const options = project
    ? [
        ...(workspace.available && !workspace.locked && workspace.id !== project.id
          ? [{ key: "switch", label: "Switch to this workspace" }]
          : []),
        ...(platform.openProjectWindow && !platform.windowProject
          ? [{ key: "window", label: "Open in new window" }]
          : []),
        { key: "edit", label: "Edit…" },
        ...(platform.openFolder && project.folder ? [{ key: "folder", label: "Open folder" }] : []),
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
    else if (key === "switch") workspace.choose(project.id);
    else if (key === "memory") onMemory();
    else if (key === "folder" && project.folder) void platform.openFolder?.(project.folder);
    else if (key === "pause")
      void (project.status === "paused"
        ? setStatus("active", "Project resumed.")
        : setStatus("paused", "Project paused."));
    else if (key === "archive") setArchiving(true);
    else if (key === "window" && project) void platform.openProjectWindow?.(project.id, project.name);
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
  return { options, pick, confirm };
}

const _DIM = "var(--precision-text3)";

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
  const active = tasks.filter(isActive).length;
  const need = waiting.approvals + waiting.decisions;
  const last = tasks.reduce((at, t) => Math.max(at, t.updated_at_ms), 0);
  const chip =
    project.status === "archived" ? (
      <Chip>Archived</Chip>
    ) : project.status === "paused" ? (
      <Chip>Paused</Chip>
    ) : need > 0 ? (
      <Chip tone="warn" dot>{`Needs you · ${need}`}</Chip>
    ) : active > 0 ? (
      <Chip tone="ok" dot>
        Working
      </Chip>
    ) : (
      <Chip>Quiet</Chip>
    );
  const { options, pick, confirm } = useProjectActions(
    project,
    onEdit,
    () => void navigate({ to: "/knowledge/$tab", params: { tab: "memory" }, search: { project: project.id } }),
  );
  return (
    <TableRow
      onClick={() => void navigate({ to: "/projects/$id", params: { id: project.id } })}
      {...stylex.props(stylex.defaultMarker(), styles.row, selected && styles.selected)}
    >
      <TableCell xstyle={selected ? styles.selectedEdge : undefined}>
        <span {...stylex.props(styles.minZero)}>
          <Link
            to="/projects/$id"
            params={{ id: project.id }}
            onClick={(e) => e.stopPropagation()}
            {...stylex.props(styles.projectName)}
          >
            {project.name}
          </Link>
          <span {...stylex.props(styles.line, styles.folder)}>{project.folder || "No folder"}</span>
        </span>
        {confirm}
      </TableCell>
      <TableCell>
        <span {...stylex.props(styles.marks)}>
          {team.slice(0, 4).map((e) => (
            <Mark key={e.id} letter={initial(e.name)} color={markColor(e.id)} size={20} radius={10} />
          ))}
          {team.length > 4 ? <span {...stylex.props(styles.moreMarks)}>+{team.length - 4}</span> : null}
        </span>
      </TableCell>
      <TableCell>{chip}</TableCell>
      <TableCell>
        <span {...stylex.props(styles.small)}>{last ? agoText(last, Date.now()) : "—"}</span>
      </TableCell>
      <TableCell>
        {/* biome-ignore lint/a11y/noStaticElementInteractions: only stops the row from opening. */}
        {/* biome-ignore lint/a11y/useKeyWithClickEvents: as above. */}
        <span onClick={(e) => e.stopPropagation()} {...stylex.props(styles.actions)}>
          {project.status === "archived" ? null : (
            <button
              type="button"
              aria-label={`${pinned ? "Unpin" : "Pin"} ${project.name}`}
              aria-pressed={pinned}
              title={pinned ? "Unpin from your rail" : "Pin to your rail"}
              onClick={onPin}
              {...stylex.props(styles.pin, pinned && styles.pinned)}
            >
              {pinned ? "★" : "☆"}
            </button>
          )}
          <Menu
            label={`More for ${project.name}`}
            options={options}
            onPick={pick}
            trigger={<IconButton size="sm" variant="ghost" label={`More for ${project.name}`} icon="⋯" />}
          />
        </span>
      </TableCell>
    </TableRow>
  );
}

type WorkspaceFilter = "active" | "paused" | "archived";

/** Settings › Company › Workspaces: every workspace, to find, filter and open its own settings. */
export function ProjectsScreen() {
  const navigate = useNavigate();
  const projects = useProjects();
  const tasks = useOpenTasks();
  const rail = useRailPins();
  const [adding, setAdding] = useState(false);
  const [selected, setSelected] = useState(0);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WorkspaceFilter>("active");
  const all = projects.data ?? [];
  const countOf = (f: WorkspaceFilter) => all.filter((p) => p.status === f).length;
  const list = useMemo(
    () =>
      pinnedFirst(
        all.filter(
          (p) =>
            p.status === filter && (query.trim() === "" || p.name.toLowerCase().includes(query.trim().toLowerCase())),
        ),
        rail.pins,
      ),
    [all, filter, query, rail.pins],
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
    <SettingsPage title="Workspaces" width="wide">
      <Toolbar>
        <FindField label="Search workspaces" placeholder="Search workspaces" value={query} onChange={setQuery} grow />
        <Segments
          label="Show"
          options={[
            { value: "active", label: `Active ${countOf("active")}` },
            { value: "paused", label: `Paused ${countOf("paused")}` },
            { value: "archived", label: `Archived ${countOf("archived")}` },
          ]}
          value={filter}
          onChange={setFilter}
        />
        <Grow />
        <Button size="sm" variant="primary" label="New workspace" onClick={() => setAdding(true)} />
      </Toolbar>
      {projects.data && all.length === 0 ? (
        <Empty
          title="Begin with what is in your control."
          description="Create a workspace to give an employee a mandate."
        >
          <Button variant="primary" label="New workspace" onClick={() => setAdding(true)} />
        </Empty>
      ) : null}
      {all.length > 0 ? (
        <div {...stylex.props(styles.card)}>
          <Table density="compact" hasHover xstyle={styles.table}>
            <TableHeader>
              <TableRow isHeaderRow>
                <TableHeaderCell xstyle={styles.headCell}>Workspace</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.peopleColumn]}>People</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.statusColumn]}>Status</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.activeColumn]}>Last active</TableHeaderCell>
                <TableHeaderCell xstyle={styles.moreColumn} />
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={5}>
                    <span {...stylex.props(styles.small)}>No workspace matches.</span>
                  </TableCell>
                </TableRow>
              ) : null}
              {list.map((p, i) => (
                <ProjectRow
                  key={p.id}
                  project={p}
                  tasks={forProject(p.id)}
                  waiting={waitingOf(p.id)}
                  selected={i === selected}
                  pinned={rail.pins.includes(p.id)}
                  onEdit={() => void navigate({ to: "/projects/$id", params: { id: p.id } })}
                  onPin={() => rail.toggle(p.id)}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      ) : null}
      <Lede>A row opens the workspace’s own settings: name, folder, dispatcher, memory and export, archive.</Lede>
      {adding ? <NewProject onClose={() => setAdding(false)} /> : null}
    </SettingsPage>
  );
}

type ProjectTab = "overview" | "board" | "tasks" | "vault";
const TABS: { key: ProjectTab; label: string; feature?: Feature }[] = shown([
  { key: "overview", label: "Settings" },
  { key: "board", label: "Board" },
  { key: "tasks", label: "Tasks" },
  { key: "vault", label: "Vault", feature: "Vault" },
]);

/** A hidden tab in a URL opens the overview. */
export const projectTabShown = (tab: string): boolean => TABS.some((t) => t.key === tab);

// These two are the sidebar's screens, narrowed to the workspace, in a project window too.
const SIDEBAR_SCREENS: ProjectTab[] = ["board", "tasks"];
const PAGE_TABS = TABS.filter((t) => !SIDEBAR_SCREENS.includes(t.key));

export function ProjectPage({ id, tab: asked }: { id: string; tab: ProjectTab }) {
  const navigate = useNavigate();
  const tabs = PAGE_TABS;
  const tab = tabs.some((t) => t.key === asked) ? asked : "overview";
  const projects = useProjects();
  const project = projects.data?.find((p) => p.id === id) ?? null;
  const [changingFolder, setChangingFolder] = useState(false);
  const [archiving, setArchiving] = useState(false);
  const { pick, confirm } = useProjectActions(
    project,
    () => focusField("Name"),
    () =>
      void navigate({
        to: "/knowledge/$tab",
        params: { tab: "memory" },
        search: project ? { project: project.id } : {},
      }),
  );
  const settings = () => {
    if (tab === "overview") focusField("Name");
    else void navigate({ to: "/projects/$id", params: { id } });
  };
  const go = (delta: number) => {
    const at = tabs.findIndex((t) => t.key === tab);
    const next = tabs[(at + delta + tabs.length) % tabs.length];
    if (next) void navigate({ to: "/projects/$id", params: { id }, search: { tab: next.key } });
  };
  useAction("NextTab", () => go(1));
  useAction("PrevTab", () => go(-1));
  useAction("Back", () => void navigate({ to: "/projects" }));
  useAction("OpenProjectSettings", settings, project !== null);
  useAction("EditItem", settings, project !== null);
  useAction("ChangeProjectFolder", () => setChangingFolder(true), project !== null && tab === "overview");
  useAction("ExportProject", () => pick("export"), project !== null);
  useAction("TogglePause", () => pick("pause"), project !== null);
  useAction("ArchiveItem", () => setArchiving(true), project !== null && tab === "overview");
  useAction("OpenMemory", () => pick("memory"), project !== null);
  useAction("OpenProjectFolder", () => pick("folder"), project !== null);
  useEffect(() => {
    if (projects.data && project?.status === "archived") void navigate({ to: "/projects" });
  }, [projects.data, project, navigate]);
  if (!project) {
    return (
      <Page>
        <PageHead
          title={
            <Link to="/projects" {...stylex.props(styles.back)}>
              Back to projects
            </Link>
          }
        />
        {projects.data ? <p {...stylex.props(styles.missing)}>This project no longer exists.</p> : null}
      </Page>
    );
  }
  return (
    <Page>
      {tabs.length > 1 ? (
        <PageHead
          title={project.name}
          tabs={
            <TabList aria-label="Project" size="sm" value={tab} onChange={() => {}}>
              {tabs.map((t) => (
                <Tab key={t.key} value={t.key} label={t.label} href={`/projects/${id}?tab=${t.key}`} />
              ))}
            </TabList>
          }
        />
      ) : null}
      {tab === "overview" ? (
        <WorkspaceSettingsBody
          project={project}
          changingFolder={changingFolder}
          onChangingFolder={setChangingFolder}
          archiving={archiving}
          onArchiving={setArchiving}
        />
      ) : null}
      {tab === "vault" ? (
        <div {...stylex.props(styles.fill)}>
          <VaultBody projectId={project.id} scopeName={project.name} />
        </div>
      ) : null}
      {confirm}
    </Page>
  );
}
