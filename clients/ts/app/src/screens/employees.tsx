import { Button } from "@astryxdesign/core/Button";
import { TextArea } from "@astryxdesign/core/TextArea";
import { ProtocolError } from "@aurelius/client";
import type { EmployeeSummary, ProposalView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { type DragEvent, useCallback, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEffectiveSets, useEmployees, useOpenTasks, useProposals } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { EmployeeDialog } from "./employee-dialog";
import {
  canPin,
  canReportTo,
  type EmployeeFilter,
  employeeFilterCounts,
  employeeGroups,
  employeeState,
  inEmployeeFilter,
  matchesSearch,
  type OrgNode,
  orgTree,
  position,
  togglePin,
  workOf,
} from "./employee-logic";
import { lineDiff } from "./guideline-logic";
import { Eyebrow, FormDialog, failureOf, Hint, Problem } from "./org-kit";
import { FindField, Grow, Segments, SettingsPage, Status, type StatusTone, Toolbar } from "./settings-frame";

const LINE = "var(--color-border-emphasized)";

const styles = stylex.create({
  proposal: {
    display: "flex",
    maxWidth: 720,
    flexDirection: "column",
    gap: 8,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--precision-acc-line)",
    backgroundColor: "var(--color-background-surface)",
    paddingInline: 18,
    paddingBlock: 16,
  },
  proposalTitle: { margin: 0, fontSize: 15, fontWeight: 600 },
  secondary: { margin: 0, lineHeight: 1.5, color: "var(--color-text-secondary)" },
  clamp: {
    display: "-webkit-box",
    overflow: "hidden",
    whiteSpace: "pre-wrap",
    WebkitBoxOrient: "vertical",
    WebkitLineClamp: 4,
  },
  diff: {
    margin: 0,
    maxHeight: 240,
    overflow: "auto",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    padding: 10,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    lineHeight: "17px",
    whiteSpace: "pre-wrap",
  },
  added: { color: "var(--color-text-accent)" },
  removed: { color: "var(--color-text-error)" },
  same: { color: "var(--color-text-secondary)" },
  warning: { margin: 0, color: "var(--color-text-warning)" },
  buttons: { display: "flex", justifyContent: "flex-end", gap: 8 },
  scroll: { minHeight: 0, flex: 1, overflow: "auto" },
  note: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  table: { minWidth: 560 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  // A cell has max-width: 0 for truncating, which would close these columns.

  // A cell has max-width: 0 for truncating, which would close these columns.
  statusColumn: { width: 150, minWidth: 150, maxWidth: 150 },
  guidelinesColumn: { width: 90, minWidth: 90, maxWidth: 90 },
  groupCell: {
    height: 30,
    backgroundColor: "var(--precision-sunk)",
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-secondary)",
  },
  card: {
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    backgroundColor: "var(--color-background-card)",
    boxShadow: "var(--precision-card-shadow)",
  },
  hdr: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--color-border)",
    paddingBlock: 8,
    paddingInline: 16,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  who: { display: "flex", width: 230, minWidth: 0, flexShrink: 0, alignItems: "center", gap: 10 },
  stc: { width: 140, flexShrink: 0 },
  on: {
    minWidth: 0,
    flex: 1,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--color-text-secondary)",
  },
  faint: { color: "var(--precision-text3)" },
  gl: {
    width: 92,
    flexShrink: 0,
    fontSize: 12,
    textAlign: "end",
    whiteSpace: "nowrap",
    color: "var(--precision-text3)",
  },
  pinCol: { width: 26, flexShrink: 0 },
  gh: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    paddingBlock: "14px 6px",
    paddingInline: 16,
    fontSize: 11.5,
    fontWeight: 600,
    letterSpacing: "0.03em",
    textTransform: "uppercase",
    color: "var(--precision-text3)",
  },
  ghCount: { fontWeight: 500 },
  emp: {
    display: "flex",
    minHeight: 50,
    boxSizing: "border-box",
    alignItems: "center",
    gap: 12,
    cursor: "pointer",
    paddingBlock: 9,
    paddingInline: 16,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
  },
  first: { borderBlockStartWidth: 0 },
  selected: { boxShadow: "inset 2px 0 0 var(--color-text-accent)", backgroundColor: "var(--color-overlay-hover)" },
  names: { display: "flex", minWidth: 0, flexDirection: "column" },
  name: {
    overflow: "hidden",
    fontWeight: 600,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--color-text-primary)",
    textDecoration: { default: "none", ":hover": "underline" },
  },
  role: {
    overflow: "hidden",
    fontSize: 12,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--precision-text3)",
  },
  pin: {
    display: "grid",
    width: 26,
    height: 26,
    flexShrink: 0,
    placeItems: "center",
    cursor: "pointer",
    borderWidth: 0,
    borderRadius: 6,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-pressed)" },
    fontSize: 14,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  pinned: { color: "#c08a00" },
  none: { margin: 0, padding: 16, color: "var(--precision-text3)" },
  org: { overflowX: "auto", paddingBlock: "22px 26px", paddingInline: 16 },
  roots: {
    display: "flex",
    width: "max-content",
    minWidth: "100%",
    justifyContent: "center",
    gap: 28,
    margin: 0,
    padding: 0,
  },
  branch: { display: "flex", listStyleType: "none", flexDirection: "column", alignItems: "center" },
  stem: { width: 1, height: 18, backgroundColor: LINE },
  children: { position: "relative", display: "flex", gap: 6, margin: 0, padding: 0 },
  child: {
    position: "relative",
    display: "flex",
    listStyleType: "none",
    flexDirection: "column",
    alignItems: "center",
  },
  bar: { position: "absolute", top: 0, height: 1, backgroundColor: LINE },
  barFirst: { insetInlineEnd: -3, insetInlineStart: "50%" },
  barLast: { insetInlineEnd: "50%", insetInlineStart: -3 },
  barMiddle: { insetInlineEnd: -3, insetInlineStart: -3 },
  bare: { margin: 0, padding: 0 },
  team: {
    display: "grid",
    gridTemplateColumns: "repeat(4, auto)",
    gap: 8,
    margin: 0,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: LINE,
    borderRadius: 10,
    padding: "8px 10px",
    listStyleType: "none",
  },
  node: {
    display: "flex",
    width: 106,
    boxSizing: "border-box",
    alignItems: "center",
    gap: 7,
    cursor: "grab",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: LINE, ":hover": "var(--precision-text3)" },
    borderRadius: 10,
    paddingBlock: 7,
    paddingInline: 8,
    backgroundColor: "var(--color-background-card)",
    color: "var(--color-text-primary)",
    textAlign: "start",
    textDecoration: "none",
  },
  nodework: { borderColor: "var(--color-text-accent)", boxShadow: "0 0 0 3px var(--color-accent-muted)" },
  nodeyou: { borderColor: "var(--color-text-warning)", boxShadow: "0 0 0 3px var(--color-warning-muted)" },
  nodepaused: { opacity: 0.55 },
  nodeDrop: { outlineWidth: 2, outlineStyle: "dashed", outlineColor: "var(--color-text-accent)", outlineOffset: 2 },
  nodeText: { display: "flex", minWidth: 0, flexDirection: "column" },
  nodeName: { fontSize: 12.5, fontWeight: 600 },
  nodeRole: {
    overflow: "hidden",
    fontSize: 11,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--precision-text3)",
  },
  proposals: { display: "flex", flexDirection: "column", alignItems: "stretch", gap: 12 },
  legend: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 14,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingBlock: 10,
    paddingInline: 16,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
});

/** Every list of pins on screen follows a change, the sidebar too. */
const pinListeners = new Set<(key: string) => void>();

const NO_PINS: string[] = [];

export function usePins(key: string) {
  const [pins, set] = useStoredPins(key);
  return [pins ?? NO_PINS, set] as const;
}

/** The pins under `key`, or null while nobody chose any on this device: a default may then apply. */
export function useStoredPins(key: string) {
  const platform = usePlatform();
  const read = useCallback(() => {
    const raw = platform.storage.get(key);
    if (raw === null) return null;
    try {
      const value: unknown = JSON.parse(raw);
      return Array.isArray(value) ? value.filter((x): x is string => typeof x === "string") : [];
    } catch {
      return [];
    }
  }, [key, platform]);
  const [pins, setPins] = useState<string[] | null>(read);
  useEffect(() => {
    const changed = (other: string) => {
      if (other === key) setPins(read());
    };
    pinListeners.add(changed);
    return () => {
      pinListeners.delete(changed);
    };
  }, [key, read]);
  const set = (next: string[]) => {
    platform.storage.set(key, JSON.stringify(next));
    for (const listener of pinListeners) listener(key);
  };
  return [pins, set] as const;
}

export const pinKey = (server: string | null, what: string) => `${what}:${server ?? ""}`;

function ProposalCard({ proposal, all }: { proposal: ProposalView; all: EmployeeSummary[] }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(proposal.instructions);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const isInstructions = proposal.kind === "instructions";
  const by = proposal.proposed_by_name ?? "An employee";
  const target =
    proposal.target_employee_name ?? all.find((e) => e.id === proposal.target_employee_id)?.name ?? "this employee";
  const self = proposal.target_employee_id === proposal.proposed_by;
  const stale =
    isInstructions &&
    proposal.base_version != null &&
    proposal.current_version != null &&
    proposal.current_version !== proposal.base_version;
  const decide = async (approve: boolean, instructions?: string) => {
    setBusy(true);
    setProblem(null);
    try {
      await connection.write(
        "proposal.decide",
        { proposal_id: proposal.id, approve, ...(instructions === undefined ? {} : { instructions }) },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["proposals"] });
      await client.invalidateQueries({ queryKey: ["employees"] });
      setEditing(false);
    } catch (e) {
      setProblem(
        e instanceof ProtocolError && e.code === "version_conflict"
          ? "The instructions changed since this was proposed, so it can't be approved as it is. Decline it, or edit the instructions in Edit employee."
          : failureOf(e),
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <article {...stylex.props(styles.proposal)}>
      <Eyebrow>{isInstructions ? "INSTRUCTIONS PROPOSED" : "COLLEAGUE PROPOSED"} · WAITING FOR YOU</Eyebrow>
      <h2 {...stylex.props(styles.proposalTitle)}>
        {isInstructions
          ? self
            ? `${by} proposes new instructions for itself`
            : `${by} proposes new instructions for ${target}`
          : `${by} proposes ${proposal.name}`}
      </h2>
      {isInstructions ? null : (
        <p {...stylex.props(styles.secondary)}>
          {[proposal.title, proposal.role, proposal.reports_to_name ? `reports to ${proposal.reports_to_name}` : null]
            .filter(Boolean)
            .join(" · ")}
        </p>
      )}
      {proposal.reason ? (
        <>
          <Eyebrow>WHY</Eyebrow>
          <p {...stylex.props(styles.secondary)}>{proposal.reason}</p>
        </>
      ) : null}
      {isInstructions ? (
        <>
          <Eyebrow>CHANGE · NOW → PROPOSED</Eyebrow>
          <pre {...stylex.props(styles.diff)}>
            {lineDiff(proposal.current_instructions ?? "", proposal.instructions).map((l, i) => (
              <div
                // biome-ignore lint/suspicious/noArrayIndexKey: a diff is drawn whole.
                key={i}
                {...stylex.props(l.mark === "+" ? styles.added : l.mark === "-" ? styles.removed : styles.same)}
              >
                {l.mark} {l.line}
              </div>
            ))}
          </pre>
        </>
      ) : (
        <>
          <Eyebrow>INSTRUCTIONS</Eyebrow>
          <p {...stylex.props(styles.secondary, styles.clamp)}>{proposal.instructions}</p>
        </>
      )}
      {stale ? (
        <Problem>
          {target}'s instructions changed since this was proposed (version {proposal.base_version} →{" "}
          {proposal.current_version}), so it can't be approved. Decline it; the proposer can propose again on the new
          text.
        </Problem>
      ) : null}
      {problem ? <Problem>{problem}</Problem> : null}
      <div {...stylex.props(styles.buttons)}>
        <Button size="sm" label="Decline" isDisabled={busy} onClick={() => void decide(false)} />
        {isInstructions && !stale ? (
          <Button size="sm" label="Edit and approve…" isDisabled={busy} onClick={() => setEditing(true)} />
        ) : null}
        {stale ? null : (
          <Button size="sm" variant="primary" label="Approve" isDisabled={busy} onClick={() => void decide(true)} />
        )}
      </div>
      {editing ? (
        <FormDialog
          title="Edit and approve instructions"
          width={620}
          onClose={() => setEditing(false)}
          footer={
            <>
              <Button label="Cancel" onClick={() => setEditing(false)} />
              <Button
                variant="primary"
                label="Approve these"
                isDisabled={busy || text.trim() === ""}
                onClick={() => void decide(true, text)}
              />
            </>
          }
        >
          <TextArea hasAutoFocus label="Instructions" rows={12} value={text} onChange={setText} />
          <Hint>
            This text replaces the employee's instructions as a new version; the proposer is told it was edited.
          </Hint>
          {text.trim() === "" ? (
            <p {...stylex.props(styles.warning)}>Write the instructions to approve, or decline the proposal instead.</p>
          ) : null}
        </FormDialog>
      ) : null}
    </article>
  );
}

/** How an employee's state reads in a list: the word, and the tone of its dot. */
function statusOf(e: EmployeeSummary): { tone: StatusTone; label: string } {
  if (e.status === "archived") return { tone: "paused", label: "Archived" };
  const s = employeeState(e);
  const tone: StatusTone =
    s.label === "Working"
      ? "work"
      : s.label === "Waiting for you"
        ? "you"
        : s.label === "Paused"
          ? "paused"
          : s.label === "Queued"
            ? "wait"
            : "plain";
  return { tone, label: s.label };
}

type View = "list" | "org";

/** Settings › Company › Employees: the list or the org chart, with find, filters and Add employee. */
export function EmployeesScreen({ view = "list" }: { view?: View } = {}) {
  const { hostName } = useLinkState();
  const navigate = useNavigate();
  const employees = useEmployees();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<EmployeeFilter>("all");
  const [adding, setAdding] = useState(false);
  useAction("AddEmployee", () => setAdding(true));
  const all = employees.data ?? [];
  const counts = employeeFilterCounts(all);
  const shown = all.filter((e) => inEmployeeFilter(e, filter) && matchesSearch(e, query));
  const filters: { value: EmployeeFilter; label: string }[] = [
    { value: "all", label: `All ${counts.all}` },
    { value: "work", label: `Working ${counts.work}` },
    { value: "you", label: `Waiting for you ${counts.you}` },
    { value: "paused", label: `Paused ${counts.paused}` },
    ...(counts.archived > 0 ? [{ value: "archived" as const, label: `Archived ${counts.archived}` }] : []),
  ];
  return (
    <SettingsPage title="Employees" width="wide">
      <Toolbar>
        <FindField label="Search employees" placeholder="Search by name or role" value={query} onChange={setQuery} />
        <Segments label="Show" options={filters} value={filter} onChange={setFilter} />
        <Grow />
        <Segments
          label="View"
          options={[
            { value: "list", label: "List" },
            { value: "org", label: "Org chart" },
          ]}
          value={view}
          onChange={(next) => void navigate({ to: next === "org" ? "/employees/org" : "/employees" })}
        />
        <Button size="sm" variant="primary" label="Add employee" onClick={() => setAdding(true)} />
      </Toolbar>
      {view === "org" ? (
        <OrgChart all={all} shown={new Set(shown.map((e) => e.id))} />
      ) : (
        <EmployeeList all={all} shown={shown} server={hostName} loaded={employees.data !== undefined} />
      )}
      {adding ? <EmployeeDialog employee={null} onClose={() => setAdding(false)} /> : null}
    </SettingsPage>
  );
}

function EmployeeList({
  all,
  shown,
  server,
  loaded,
}: {
  all: EmployeeSummary[];
  shown: EmployeeSummary[];
  server: string | null;
  loaded: boolean;
}) {
  const navigate = useNavigate();
  const [selected, setSelected] = useState<string | null>(null);
  const [pins, setPins] = usePins(pinKey(server, "pinned_employees"));
  const groups = useMemo(() => employeeGroups(all, shown, pins), [all, shown, pins]);
  const pairs = useMemo(() => all.map((e): [string, null] => [e.id, null]), [all]);
  const sets = useEffectiveSets(pairs);
  const flat = groups.flatMap((g) => g.list);
  const index = flat.findIndex((e) => e.id === selected);
  const go = (delta: number) => {
    const next = flat[Math.max(0, Math.min(flat.length - 1, index + delta))];
    if (next) setSelected(next.id);
  };
  useAction("SelectNext", () => go(1));
  useAction("SelectPrev", () => go(-1));
  useAction(
    "OpenSelected",
    () => {
      if (selected) void navigate({ to: "/employees/$id", params: { id: selected } });
    },
    selected !== null,
  );
  return (
    <div {...stylex.props(styles.card)}>
      <div {...stylex.props(styles.hdr)}>
        <span {...stylex.props(styles.who)}>Name</span>
        <span {...stylex.props(styles.stc)}>Status</span>
        <span {...stylex.props(styles.on)}>Working on</span>
        <span {...stylex.props(styles.gl)}>Guidelines</span>
        <span {...stylex.props(styles.pinCol)} />
      </div>
      {loaded && all.length === 0 ? <p {...stylex.props(styles.none)}>No employees yet.</p> : null}
      {all.length > 0 && flat.length === 0 ? <p {...stylex.props(styles.none)}>Nobody matches.</p> : null}
      {groups.map((g) => (
        <section key={g.label} aria-label={g.label}>
          <div {...stylex.props(styles.gh)}>
            {g.label} <span {...stylex.props(styles.ghCount)}>· {g.list.length}</span>
          </div>
          {g.list.map((e, i) => {
            const status = statusOf(e);
            const work = workOf(e.agents);
            const set = sets.data?.get(`${e.id}|`);
            const pinned = pins.includes(e.id);
            return (
              // biome-ignore lint/a11y/useKeyWithClickEvents: the name inside is the link; the row is a larger target.
              // biome-ignore lint/a11y/useSemanticElements: one employee as a named group of cells; a fieldset is for form controls.
              <div
                key={e.id}
                role="group"
                aria-label={e.name}
                onClick={() => {
                  setSelected(e.id);
                  void navigate({ to: "/employees/$id", params: { id: e.id } });
                }}
                {...stylex.props(styles.emp, i === 0 && styles.first, selected === e.id && styles.selected)}
              >
                <span {...stylex.props(styles.who)}>
                  <Mark letter={initial(e.name)} color={markColor(e.id)} size={24} radius={6} />
                  <span {...stylex.props(styles.names)}>
                    <Link to="/employees/$id" params={{ id: e.id }} {...stylex.props(styles.name)}>
                      {e.name}
                    </Link>
                    <span {...stylex.props(styles.role)}>{position(e)}</span>
                  </span>
                </span>
                <span {...stylex.props(styles.stc)}>
                  <Status tone={status.tone} label={status.label} />
                </span>
                <span {...stylex.props(styles.on, !work && styles.faint)}>
                  {e.status === "archived" ? "History and conversations are kept" : (work?.title ?? "—")}
                </span>
                <span {...stylex.props(styles.gl)}>
                  {set ? `${set.guidelines.length} · ${set.total_chars.toLocaleString()} ch` : ""}
                </span>
                {e.status === "archived" ? (
                  <span {...stylex.props(styles.pinCol)} />
                ) : (
                  <button
                    type="button"
                    aria-label={pinned ? `Unpin ${e.name}` : `Pin ${e.name}`}
                    aria-pressed={pinned}
                    onClick={(ev) => {
                      ev.stopPropagation();
                      if (!canPin(pins, e.id)) return toast("Only 5 employees can be pinned. Unpin one first.");
                      setPins(togglePin(pins, e.id));
                    }}
                    {...stylex.props(styles.pin, pinned && styles.pinned)}
                  >
                    {pinned ? "★" : "☆"}
                  </button>
                )}
              </div>
            );
          })}
        </section>
      ))}
    </div>
  );
}

type OrgTone = "work" | "you" | "paused" | "plain";

/** From its open tasks: whoever must act next. Actually working goes first. */
function orgTone(e: EmployeeSummary, tasks: TaskView[]): OrgTone {
  if (e.status === "paused") return "paused";
  if (e.working) return "work";
  const mine = tasks.filter((t) => t.employee_id === e.id);
  if (mine.some((t) => t.pending_approvals > 0 || (t.status === "waiting" && t.waiting_reason === "input")))
    return "you";
  return statusOf(e).tone === "you" ? "you" : "plain";
}

const NODE_TONES = { work: styles.nodework, you: styles.nodeyou, paused: styles.nodepaused, plain: null } as const;

interface Drag {
  dragging: string | null;
  over: string | null;
  start: (id: string) => void;
  enter: (id: string, event: DragEvent) => void;
  leave: () => void;
  drop: (id: string) => void;
  end: () => void;
}

function OrgCard({ node, tasks, drag }: { node: OrgNode; tasks: TaskView[]; drag: Drag }) {
  const e = node.employee;
  const tone = orgTone(e, tasks);
  return (
    <Link
      to="/employees/$id"
      params={{ id: e.id }}
      title={`${e.name} · ${position(e)}`}
      draggable
      onDragStart={(ev) => {
        ev.dataTransfer.effectAllowed = "move";
        ev.dataTransfer.setData("text/plain", e.id);
        drag.start(e.id);
      }}
      onDragOver={(ev) => drag.enter(e.id, ev)}
      onDragLeave={drag.leave}
      onDrop={(ev) => {
        ev.preventDefault();
        drag.drop(e.id);
      }}
      onDragEnd={drag.end}
      {...stylex.props(styles.node, NODE_TONES[tone], drag.over === e.id && styles.nodeDrop)}
    >
      <Mark letter={initial(e.name)} color={markColor(e.id)} size={20} radius={5} />
      <span {...stylex.props(styles.nodeText)}>
        <span {...stylex.props(styles.nodeName)}>{e.name}</span>
        <span {...stylex.props(styles.nodeRole)}>{e.status === "paused" ? "Paused" : (e.title ?? e.role)}</span>
      </span>
    </Link>
  );
}

/** A card with its reports below; a big flat team sits in a box of four across, as in the design. */
function OrgBranch({ node, tasks, drag }: { node: OrgNode; tasks: TaskView[]; drag: Drag }) {
  const team = node.children.length > 4 && node.children.every((c) => c.children.length === 0);
  return (
    <li {...stylex.props(styles.branch)}>
      <OrgCard node={node} tasks={tasks} drag={drag} />
      {node.children.length > 0 ? <span {...stylex.props(styles.stem)} /> : null}
      {team ? (
        <ul {...stylex.props(styles.team)}>
          {node.children.map((c) => (
            <li key={c.employee.id} {...stylex.props(styles.branch)}>
              <OrgCard node={c} tasks={tasks} drag={drag} />
            </li>
          ))}
        </ul>
      ) : node.children.length > 0 ? (
        <ul {...stylex.props(styles.children)}>
          {node.children.map((c, i) => (
            <li key={c.employee.id} {...stylex.props(styles.child)}>
              {node.children.length === 1 ? null : (
                <span
                  {...stylex.props(
                    styles.bar,
                    i === 0 ? styles.barFirst : i === node.children.length - 1 ? styles.barLast : styles.barMiddle,
                  )}
                />
              )}
              <span {...stylex.props(styles.stem)} />
              <ul {...stylex.props(styles.bare)}>
                <OrgBranch node={c} tasks={tasks} drag={drag} />
              </ul>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}

/** Prune a tree to what the find field and filter leave, keeping the bosses on the way down. */
function prune(nodes: OrgNode[], keep: Set<string>): OrgNode[] {
  return nodes.flatMap((n) => {
    const children = prune(n.children, keep);
    return keep.has(n.employee.id) || children.length > 0 ? [{ ...n, children }] : [];
  });
}

function OrgChart({ all, shown }: { all: EmployeeSummary[]; shown: Set<string> }) {
  const connection = useConnection();
  const client = useQueryClient();
  const tasks = useOpenTasks();
  const open = tasks.data ?? [];
  const tree = useMemo(() => prune(orgTree(all), shown), [all, shown]);
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const drag: Drag = {
    dragging,
    over,
    start: setDragging,
    enter: (id, event) => {
      if (!dragging || !canReportTo(all, dragging, id)) return setOver(null);
      event.preventDefault();
      setOver(id);
    },
    leave: () => setOver(null),
    drop: (boss) => {
      const employee = dragging;
      setDragging(null);
      setOver(null);
      if (!employee || !canReportTo(all, employee, boss)) return;
      const name = (id: string) => all.find((e) => e.id === id)?.name ?? "";
      connection
        .write("employee.set_reports_to", { employee_id: employee, reports_to: boss }, { retry: false })
        .then(() => {
          toast(`${name(employee)} now reports to ${name(boss)}.`);
          return client.invalidateQueries({ queryKey: ["employees"] });
        })
        .catch((e: unknown) => toast(failureOf(e)));
    },
    end: () => {
      setDragging(null);
      setOver(null);
    },
  };
  const proposals = useProposals();
  const pending = (proposals.data ?? []).filter((p) => p.status === "pending");
  return (
    <>
      {pending.length > 0 ? (
        <div {...stylex.props(styles.proposals)}>
          {pending.map((p) => (
            <ProposalCard key={p.id} proposal={p} all={all} />
          ))}
        </div>
      ) : null}
      <div {...stylex.props(styles.card)}>
        <div {...stylex.props(styles.org)}>
          {tree.length === 0 ? <p {...stylex.props(styles.none)}>Nobody matches.</p> : null}
          <ul {...stylex.props(styles.roots)}>
            {tree.map((n) => (
              <OrgBranch key={n.employee.id} node={n} tasks={open} drag={drag} />
            ))}
          </ul>
        </div>
        <div {...stylex.props(styles.legend)}>
          <Status tone="work" label="Working" />
          <Status tone="you" label="Waiting for you" />
          <Status tone="paused" label="Paused" />
          <Grow />
          <span>Drag a card onto another employee to change who it reports to.</span>
        </div>
      </div>
    </>
  );
}

/** The org chart is a view of Employees: /employees/org opens it. */
export function OrgChartScreen() {
  return <EmployeesScreen view="org" />;
}
