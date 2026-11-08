import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Card } from "@astryxdesign/core/Card";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import type { ProjectSummary, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
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
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { BoardScreen } from "./board";
import { canPin, pinnedFirst, togglePin } from "./employee-logic";
import { pinKey, usePins } from "./employees";
import { IdeasBody } from "./ideas";
import { splitIdeas } from "./knowledge-logic";
import {
  Confirm,
  DialogSelector,
  Empty,
  Eyebrow,
  FormDialog,
  failureOf,
  Hint,
  Loading,
  Page,
  PageHead,
  Problem,
} from "./org-kit";
import { overlays } from "./overlays-store";
import { useFolderChooser } from "./run-tools";
import { ScheduleDialog } from "./settings-advanced";
import { TasksScreen } from "./tasks";
import { taskStateLine } from "./tasks-logic";
import { VaultBody } from "./vault";

// Selector takes no empty value, so "the company's default dispatcher" needs a word of its own.
const COMPANY_DEFAULT = "company-default";

const styles = stylex.create({
  section: { display: "flex", flexDirection: "column", gap: 4 },
  withAction: { display: "flex", alignItems: "flex-end", gap: 8 },
  grow: { minWidth: 0, flex: 1 },
  mono: { fontFamily: "var(--font-family-code)" },
  buttons: { display: "flex", gap: 8 },
  start: { alignSelf: "flex-start" },
  label: { fontSize: 12, color: "var(--precision-text3)" },
  folder: { fontFamily: "var(--font-family-code)", color: "var(--color-text-secondary)" },
  tooLong: { fontSize: 12, color: "var(--color-text-error)" },
  warning: { margin: 0, fontSize: 12, color: "var(--color-text-warning)" },
  scroll: { minHeight: 0, flex: 1, overflow: "auto" },
  table: { minWidth: 760 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  peopleColumn: { width: 180 },
  waitingColumn: { width: 120 },
  activeColumn: { width: 110 },
  statusColumn: { width: 110 },
  moreColumn: { width: 64 },
  row: { height: 60 },
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
  overview: {
    display: "grid",
    minHeight: 0,
    flex: 1,
    gridAutoRows: "min-content",
    gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))",
    gap: 16,
    overflowY: "auto",
    padding: 22,
  },
  wide: { gridColumn: "1 / -1" },
  card: { display: "flex", minWidth: 0, flexDirection: "column", overflow: "hidden" },
  cardHead: {
    display: "flex",
    height: 40,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
    paddingInline: 14,
  },
  cardTitle: { minWidth: 0, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  cardRight: {
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    marginInlineStart: "auto",
    fontSize: 12,
    whiteSpace: "nowrap",
  },
  cardBody: { display: "flex", flexDirection: "column" },
  rowLink: {
    display: "flex",
    minHeight: 42,
    alignItems: "center",
    gap: 10,
    borderBlockEndWidth: { default: 1, ":last-child": 0 },
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 14,
    color: "var(--color-text-primary)",
    textDecoration: "none",
  },
  rowText: { minWidth: 0, flex: 1, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  rowName: {
    minWidth: 0,
    flex: 1,
    overflow: "hidden",
    fontWeight: 500,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  note: { margin: 0, padding: 14, color: "var(--precision-text3)" },
  ticket: { marginInlineEnd: 8, fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--precision-text3)" },
  rowSide: { flexShrink: 0, fontSize: 12 },
  rowDim: { flexShrink: 0, fontSize: 12, color: "var(--precision-text3)" },
  rowMono: { fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--precision-text3)" },
  on: { color: "var(--color-text-accent)" },
  dim: { color: "var(--precision-text3)" },
  link: { color: "var(--color-text-accent)", textDecoration: "none" },
  cardFoot: { padding: 10 },
  added: {
    margin: 0,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    padding: 14,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  missing: { margin: 0, padding: 24, color: "var(--color-text-secondary)" },
  back: { color: "var(--color-text-secondary)", textDecoration: "none" },
  chevron: {
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
    textDecoration: "none",
  },
  truncatedName: { overflow: "hidden", fontWeight: 600, whiteSpace: "nowrap", textOverflow: "ellipsis" },
  path: {
    maxWidth: 260,
    overflow: "hidden",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  fill: { display: "flex", minHeight: 0, flex: 1 },
});

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
    <FormDialog
      title={`Edit project · ${project.name}`}
      width={720}
      top
      onClose={onClose}
      footer={<Button label="Close" onClick={onClose} />}
    >
      <Eyebrow>GENERAL</Eyebrow>
      <div {...stylex.props(styles.withAction)}>
        <span {...stylex.props(styles.grow)}>
          <TextInput
            width="100%"
            hasAutoFocus
            label="Name"
            placeholder="Project name"
            value={name}
            onChange={setName}
          />
        </span>
        <Button
          label="Save"
          isDisabled={name.trim() === "" || name.trim() === project.name}
          onClick={() =>
            void run(() =>
              connection.write(
                "project.rename",
                { project_id: project.id, name: name.trim() },
                { retry: false, expectedVersion: project.version },
              ),
            )
          }
        />
      </div>
      {canDescribe ? (
        <div {...stylex.props(styles.section)}>
          <TextArea
            label="Description"
            rows={3}
            placeholder="What this project is about"
            value={description}
            onChange={setDescription}
          />
          <span {...stylex.props(tooLong ? styles.tooLong : styles.label)}>
            {tooLong ? `Too long: ${description.length} of at most 2000 characters.` : `${description.length} / 2000`}
          </span>
          <div>
            <Button
              label="Save"
              isDisabled={tooLong || description.trim() === (project.description ?? "")}
              onClick={() =>
                void run(() =>
                  connection.write(
                    "project.set_description",
                    { project_id: project.id, description: description.trim() },
                    { retry: false, expectedVersion: project.version },
                  ),
                )
              }
            />
          </div>
        </div>
      ) : (
        <Hint>This server is too old to edit the description; update it to change it here.</Hint>
      )}
      <div {...stylex.props(styles.section)}>
        <span {...stylex.props(styles.label)}>Folder</span>
        <code {...stylex.props(styles.folder)}>{project.folder ?? "No folder"}</code>
        <span {...stylex.props(styles.buttons)}>
          {platform.openFolder && project.folder ? (
            <Button label="Open folder" onClick={() => void platform.openFolder?.(project.folder as string)} />
          ) : null}
          <Button
            label="Choose another folder…"
            onClick={() =>
              folders.choose("Choose another folder", (path) => {
                void run(() =>
                  connection.write("project.set_folder", { project_id: project.id, path }, { retry: false }),
                );
              })
            }
          />
        </span>
        {folders.dialog}
      </div>
      <Eyebrow>BOARD</Eyebrow>
      <div {...stylex.props(styles.section)}>
        <DialogSelector
          label="Dispatcher"
          description="Chooses an owner when a ticket of this project starts without one."
          value={dispatcher || COMPANY_DEFAULT}
          options={[
            { value: COMPANY_DEFAULT, label: `Company default (${defaultName})` },
            ...(employees.data ?? [])
              .filter((e) => e.status === "active" || e.id === dispatcher)
              .map((e) => ({ value: e.id, label: e.name })),
          ]}
          onChange={(picked) => {
            const id = picked === COMPANY_DEFAULT ? "" : picked;
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
        />
        {warning ? <p {...stylex.props(styles.warning)}>{warning}</p> : null}
      </div>
      {problem ? <Problem>{problem}</Problem> : null}
    </FormDialog>
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

const DIM = "var(--precision-text3)";

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
      ? { label: "Paused", color: DIM }
      : need > 0
        ? { label: `Needs you · ${need}`, color: "var(--color-text-warning)" }
        : active > 0
          ? { label: "Working", color: "var(--color-text-accent)" }
          : { label: "Quiet", color: DIM };
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
    <TableRow {...stylex.props(stylex.defaultMarker(), styles.row, selected && styles.selected)}>
      <TableCell xstyle={selected ? styles.selectedEdge : undefined}>
        <span {...stylex.props(styles.projectCell)}>
          <Mark letter={initial(project.name)} color={markColor(project.id)} size={28} radius={7} />
          <span {...stylex.props(styles.minZero)}>
            <Link to="/projects/$id" params={{ id: project.id }} {...stylex.props(styles.projectName)}>
              {project.name}
            </Link>
            <span {...stylex.props(styles.line)}>{project.description || "No description"}</span>
          </span>
        </span>
        {confirm}
      </TableCell>
      <TableCell>
        <span {...stylex.props(styles.marks)}>
          {team.slice(0, 4).map((e) => (
            <Mark key={e.id} letter={initial(e.name)} color={markColor(e.id)} size={16} radius={4} />
          ))}
          {team.length > 4 ? <span {...stylex.props(styles.moreMarks)}>+{team.length - 4}</span> : null}
        </span>
        <span {...stylex.props(styles.line)}>{people(team.length, working)}</span>
      </TableCell>
      <TableCell>
        <span {...stylex.props(styles.small)}>{wait}</span>
      </TableCell>
      <TableCell>
        <span {...stylex.props(styles.small)}>{plural(active, "active task")}</span>
      </TableCell>
      <TableCell>
        <span {...stylex.props(styles.small)} style={{ color: status.color }}>
          {status.label}
        </span>
      </TableCell>
      <TableCell>
        <span {...stylex.props(styles.actions)}>
          <button
            type="button"
            aria-label={`${pinned ? "Unpin" : "Pin"} ${project.name}`}
            aria-pressed={pinned}
            title={pinned ? "Unpin from the sidebar" : "Pin to the sidebar"}
            onClick={onPin}
            {...stylex.props(styles.pin, pinned && styles.pinned)}
          >
            {pinned ? "★" : "☆"}
          </button>
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
    <Page>
      <PageHead title="Projects">
        <Button size="sm" variant="primary" label="New project" onClick={() => setAdding(true)} />
      </PageHead>
      <div {...stylex.props(styles.scroll)}>
        {projects.data && list.length === 0 ? (
          <Empty
            title="Begin with what is in your control."
            description="Create a project to give an employee a mandate."
          >
            <Button variant="primary" label="New project" onClick={() => setAdding(true)} />
          </Empty>
        ) : null}
        {list.length > 0 ? (
          <Table density="compact" hasHover xstyle={styles.table}>
            <TableHeader>
              <TableRow isHeaderRow>
                <TableHeaderCell xstyle={styles.headCell}>Project</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.peopleColumn]}>People</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.waitingColumn]}>Waiting</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.activeColumn]}>Active</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.statusColumn]}>Status</TableHeaderCell>
                <TableHeaderCell xstyle={styles.moreColumn} />
              </TableRow>
            </TableHeader>
            <TableBody>
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
            </TableBody>
          </Table>
        ) : null}
      </div>
      {adding ? <NewProject onClose={() => setAdding(false)} /> : null}
      {editing ? <ProjectSettings project={editing} onClose={() => setEditing(null)} /> : null}
    </Page>
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

function Panel({ title, right, children }: { title: string; right?: React.ReactNode; children: React.ReactNode }) {
  return (
    <Card padding={0} xstyle={styles.card}>
      <header {...stylex.props(styles.cardHead)}>
        <span {...stylex.props(styles.cardTitle)}>
          <Eyebrow title={title}>{title}</Eyebrow>
        </span>
        <span {...stylex.props(styles.cardRight)}>{right}</span>
      </header>
      <div {...stylex.props(styles.cardBody)}>{children}</div>
    </Card>
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
  const row = stylex.props(styles.rowLink);
  return (
    <div {...stylex.props(styles.overview)}>
      {project.status === "paused" ? (
        <div {...stylex.props(styles.wide)}>
          <Banner
            status="warning"
            title="This project is paused. No new automatic work starts."
            endContent={
              <Button
                size="sm"
                label="Resume"
                onClick={() =>
                  void connection
                    .write("project.set_status", { project_id: project.id, status: "active" }, { retry: false })
                    .then(() => client.invalidateQueries({ queryKey: ["employees"] }))
                    .then(() => toast("Project resumed."))
                }
              />
            }
          />
        </div>
      ) : null}
      <Panel
        title={`WHO WORKS HERE · ${team.length}`}
        right={
          channel ? (
            <Link to="/chat" search={{ channel: channel.conversation_id }} {...stylex.props(styles.link)}>
              Open #{channel.name}
            </Link>
          ) : null
        }
      >
        {team.length === 0 ? (
          <p {...stylex.props(styles.note)}>Nobody works here yet. Start a ticket to put someone to work.</p>
        ) : null}
        {team.map((e) => (
          <Link key={e.id} to="/employees/$id" params={{ id: e.id }} {...row}>
            <Mark letter={initial(e.name)} color={markColor(e.id)} size={22} radius={6} />
            <span {...stylex.props(styles.rowName)}>{e.name}</span>
            <span {...stylex.props(styles.rowSide, e.working ? styles.on : styles.dim)}>
              {e.status === "paused" ? "Paused" : e.working ? "Working" : "Available"}
            </span>
          </Link>
        ))}
      </Panel>
      <Panel
        title={`TASKS · ${open.length} OPEN`}
        right={
          <Link to="/projects/$id" params={{ id: project.id }} search={{ tab: "tasks" }} {...stylex.props(styles.link)}>
            All →
          </Link>
        }
      >
        {open.length === 0 ? <p {...stylex.props(styles.note)}>No open tasks.</p> : null}
        {open.slice(0, 6).map((t) => {
          const line = taskStateLine(t);
          return (
            <Link key={t.id} to="/tasks/$taskId" params={{ taskId: t.id }} {...row}>
              <span {...stylex.props(styles.rowText)}>
                {t.ticket_number ? <span {...stylex.props(styles.ticket)}>#{t.ticket_number}</span> : null}
                {t.title}
              </span>
              <span {...stylex.props(styles.rowSide)} style={{ color: line.color }}>
                {line.text}
              </span>
            </Link>
          );
        })}
        <div {...stylex.props(styles.cardFoot)}>
          <Button
            size="sm"
            label="New ticket"
            onClick={() => overlays.open("ticket", { title: "", description: "", projectId: project.id })}
          />
        </div>
      </Panel>
      <Panel
        title={`IDEAS · ${openIdeas.length} OPEN`}
        right={
          <span {...stylex.props(styles.dim)}>
            FROM YOU · {split.yours.length} · FROM EMPLOYEES · {split.employees.length}
          </span>
        }
      >
        {ideas.isPending ? <Loading rows={2} /> : null}
        {ideas.data && openIdeas.length === 0 ? (
          <p {...stylex.props(styles.note)}>No open ideas. Note one in the Ideas tab with Ctrl+N.</p>
        ) : null}
        {openIdeas.slice(0, 5).map((i) => (
          <Link key={i.id} to="/projects/$id" params={{ id: project.id }} search={{ tab: "ideas" }} {...row}>
            <span {...stylex.props(styles.rowText)}>{i.title}</span>
            <span {...stylex.props(styles.rowDim)}>{i.author}</span>
          </Link>
        ))}
      </Panel>
      <Panel
        title={`GUIDELINES FOR EVERYONE IN ${project.name.toUpperCase()}`}
        right={
          <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} {...stylex.props(styles.link)}>
            Manage links…
          </Link>
        }
      >
        {linked.length === 0 ? (
          <p {...stylex.props(styles.note)}>None: runs here get only their employee's guidelines.</p>
        ) : null}
        {linked.map((g) => (
          <Link key={g.id} to="/knowledge/guidelines/$id" params={{ id: g.id }} {...row}>
            <span {...stylex.props(styles.rowText)}>{g.title}</span>
            <span {...stylex.props(styles.rowMono)}>{g.chars}</span>
          </Link>
        ))}
        {linked.length > 0 ? (
          <p {...stylex.props(styles.added)}>
            Added to each run here: {added} characters
            {largest && largestName ? ` · largest block: ${largestName} ${largest.total_chars}` : ""}
          </p>
        ) : null}
      </Panel>
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
      <PageHead
        title={
          <>
            <Link to="/projects" aria-label="Back to projects" {...stylex.props(styles.chevron)}>
              ‹
            </Link>
            <Mark letter={initial(project.name)} color={markColor(project.id)} size={16} radius={4} />
            <span {...stylex.props(styles.truncatedName)}>{project.name}</span>
          </>
        }
        tabs={
          <TabList aria-label="Project" size="sm" value={tab} onChange={() => {}}>
            {TABS.map((t) => (
              <Tab key={t.key} value={t.key} label={t.label} href={`/projects/${id}?tab=${t.key}`} />
            ))}
          </TabList>
        }
      >
        {project.folder ? (
          <code title={project.folder} {...stylex.props(styles.path)}>
            {project.folder}
          </code>
        ) : null}
        <Button size="sm" label="Edit project" onClick={() => setEditing(true)} />
        <Menu
          label={`More for ${project.name}`}
          options={options}
          onPick={pick}
          trigger={<IconButton size="sm" label={`More for ${project.name}`} icon="⋯" />}
        />
      </PageHead>
      {tab === "overview" ? <Overview project={project} /> : null}
      {tab === "board" ? <BoardScreen ticket={undefined} projectId={project.id} /> : null}
      {tab === "tasks" ? <TasksScreen projectId={project.id} /> : null}
      {tab === "ideas" ? <IdeasBody projectId={project.id} chrome={false} /> : null}
      {tab === "vault" ? (
        <div {...stylex.props(styles.fill)}>
          <VaultBody projectId={project.id} scopeName={project.name} />
        </div>
      ) : null}
      {editing ? <ProjectSettings project={project} onClose={() => setEditing(false)} /> : null}
      {confirm}
    </Page>
  );
}
