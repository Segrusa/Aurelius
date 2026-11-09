import { Button } from "@astryxdesign/core/Button";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ProtocolError } from "@aurelius/client";
import type { EmployeeSummary, ProposalView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEffectiveSets, useEmployees, useOpenTasks, useProposals } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { useOpenChat } from "./employee-chat";
import { EmployeeDialog } from "./employee-dialog";
import {
  canPin,
  employeeGroups,
  employeeState,
  matchesSearch,
  type OrgNode,
  orgTree,
  position,
  togglePin,
  workOf,
} from "./employee-logic";
import { lineDiff } from "./guideline-logic";
import { EmployeesTabs, Eyebrow, FormDialog, failureOf, Hint, Page, PageHead, Problem } from "./org-kit";

const CONNECTOR = "var(--precision-connector)";

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
  statusColumn: { width: 150, minWidth: 150, maxWidth: 150 },
  guidelinesColumn: { width: 90, minWidth: 90, maxWidth: 90 },
  groupCell: {
    height: 30,
    backgroundColor: "var(--precision-sunk)",
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-secondary)",
  },
  row: { height: 42 },
  selected: { backgroundColor: "var(--color-overlay-hover)" },
  selectedEdge: { boxShadow: "inset 2px 0 0 var(--color-text-accent)" },
  nameCell: { display: "flex", minWidth: 0, alignItems: "center", gap: 10 },
  name: {
    minWidth: 0,
    overflow: "hidden",
    fontWeight: 500,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-primary)",
    textDecoration: { default: "none", ":hover": "underline" },
  },
  position: {
    minWidth: 0,
    overflow: "hidden",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  pin: {
    marginInlineStart: "auto",
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
    opacity: { default: 0, ":focus-visible": 1, [stylex.when.ancestor(":hover")]: 1 },
  },
  pinned: { color: "var(--color-text-accent)", opacity: 1 },
  status: { display: "flex", alignItems: "center", gap: 8, fontSize: 12 },
  work: {
    display: "block",
    overflow: "hidden",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-secondary)",
  },
  quiet: { fontSize: 12, color: "var(--precision-text3)" },
  orgCard: {
    display: "flex",
    width: 220,
    alignItems: "center",
    gap: 10,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: { default: "var(--color-background-surface)", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 12,
    paddingBlock: 10,
    color: "var(--color-text-primary)",
    textDecoration: "none",
    boxShadow: "var(--precision-card-shadow)",
  },
  orgWorking: { borderColor: "var(--precision-acc-line)" },
  orgPaused: { opacity: 0.6 },
  orgText: { minWidth: 0, flex: 1 },
  orgName: {
    display: "block",
    overflow: "hidden",
    fontWeight: 500,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  orgRole: {
    display: "block",
    overflow: "hidden",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  orgDot: { width: 7, height: 7, flexShrink: 0, borderRadius: "50%" },
  branch: { display: "flex", listStyleType: "none", flexDirection: "column", alignItems: "center" },
  stem: { width: 1, height: 22, backgroundColor: CONNECTOR },
  children: { position: "relative", display: "flex", gap: 16, margin: 0, padding: 0 },
  child: {
    position: "relative",
    display: "flex",
    listStyleType: "none",
    flexDirection: "column",
    alignItems: "center",
  },
  bar: { position: "absolute", top: 0, height: 1, backgroundColor: CONNECTOR },
  barFirst: { insetInlineEnd: -8, insetInlineStart: "50%" },
  barLast: { insetInlineEnd: "50%", insetInlineStart: -8 },
  barMiddle: { insetInlineEnd: -8, insetInlineStart: -8 },
  bare: { margin: 0, padding: 0 },
  chart: {
    minHeight: 0,
    flex: 1,
    overflow: "auto",
    paddingInline: 24,
    paddingBlock: 36,
    backgroundImage: "radial-gradient(var(--precision-dot) 1px, transparent 1px)",
    backgroundSize: "20px 20px",
  },
  proposals: { display: "flex", flexDirection: "column", alignItems: "center", gap: 12, marginBlockEnd: 24 },
  chartEmpty: { margin: 0, textAlign: "center", color: "var(--precision-text3)" },
  roots: {
    display: "flex",
    width: "max-content",
    minWidth: "100%",
    justifyContent: "center",
    gap: 28,
    margin: 0,
    padding: 0,
  },
  legend: {
    display: "flex",
    flexShrink: 0,
    flexWrap: "wrap",
    gap: 16,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingInline: 18,
    paddingBlock: 8,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  legendItem: { display: "flex", alignItems: "center", gap: 6 },
});

/** Every list of pins on screen follows a change, the sidebar too. */
const pinListeners = new Set<(key: string) => void>();

export function usePins(key: string) {
  const platform = usePlatform();
  const read = useCallback(() => {
    try {
      return JSON.parse(platform.storage.get(key) ?? "[]") as string[];
    } catch {
      return [];
    }
  }, [key, platform]);
  const [pins, setPins] = useState<string[]>(read);
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

export function EmployeesScreen() {
  const { hostName, features } = useLinkState();
  const navigate = useNavigate();
  const employees = useEmployees();
  const openChat = useOpenChat();
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [pins, setPins] = usePins(pinKey(hostName, "pinned_employees"));
  useAction("AddEmployee", () => setAdding(true));
  const all = employees.data ?? [];
  const groups = useMemo(
    () =>
      employeeGroups(
        all,
        all.filter((e) => matchesSearch(e, query)),
        pins,
      ),
    [all, query, pins],
  );
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
    <Page>
      <PageHead title="Employees" tabs={<EmployeesTabs />}>
        <TextInput
          label="Search employees"
          isLabelHidden
          size="sm"
          width={260}
          placeholder="Search employees by name, role or title"
          value={query}
          onChange={setQuery}
        />
        <Button size="sm" variant="primary" label="Add employee" onClick={() => setAdding(true)} />
      </PageHead>
      <div {...stylex.props(styles.scroll)}>
        {employees.data && all.length === 0 ? <p {...stylex.props(styles.note)}>No employees yet.</p> : null}
        {all.length > 0 && flat.length === 0 ? <p {...stylex.props(styles.note)}>Nobody matches.</p> : null}
        {flat.length > 0 ? (
          <Table density="compact" hasHover xstyle={styles.table}>
            <TableHeader>
              <TableRow isHeaderRow>
                <TableHeaderCell xstyle={styles.headCell}>Name</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.statusColumn]}>Status</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>Working on</TableHeaderCell>
                <TableHeaderCell xstyle={[styles.headCell, styles.guidelinesColumn]}>Guidelines</TableHeaderCell>
              </TableRow>
            </TableHeader>
            {groups.map((g) => (
              <TableBody key={g.label} aria-label={g.label}>
                <TableRow>
                  <TableCell colSpan={4} xstyle={styles.groupCell}>
                    {g.label} · {g.list.length}
                  </TableCell>
                </TableRow>
                {g.list.map((e) => {
                  const s = employeeState(e);
                  const work = workOf(e.agents);
                  const set = sets.data?.get(`${e.id}|`);
                  const pinned = pins.includes(e.id);
                  const isSelected = selected === e.id;
                  return (
                    <TableRow
                      key={e.id}
                      onClick={() => setSelected(e.id)}
                      {...stylex.props(stylex.defaultMarker(), styles.row, isSelected && styles.selected)}
                    >
                      <TableCell xstyle={isSelected ? styles.selectedEdge : undefined}>
                        <span {...stylex.props(styles.nameCell)}>
                          <Mark letter={initial(e.name)} color={markColor(e.id)} size={22} radius={6} />
                          <Link to="/employees/$id" params={{ id: e.id }} {...stylex.props(styles.name)}>
                            {e.name}
                          </Link>
                          <span {...stylex.props(styles.position)}>{position(e)}</span>
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
                          {features.includes("employee_chats") && e.status === "active" ? (
                            <Button
                              size="sm"
                              variant="ghost"
                              label="Chat"
                              onClick={(ev) => {
                                ev.stopPropagation();
                                openChat(e.id);
                              }}
                            />
                          ) : null}
                        </span>
                      </TableCell>
                      <TableCell>
                        <span {...stylex.props(styles.status)} style={{ color: s.color }}>
                          <StatusDot variant={s.variant} isPulsing={s.isPulsing} label={s.label} />
                          {s.label}
                        </span>
                      </TableCell>
                      <TableCell>
                        <span {...stylex.props(styles.work)}>{work?.title ?? ""}</span>
                      </TableCell>
                      <TableCell>
                        <span {...stylex.props(styles.quiet)}>
                          {set ? `${set.guidelines.length} · ${set.total_chars}` : ""}
                        </span>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            ))}
          </Table>
        ) : null}
      </div>
      {adding ? <EmployeeDialog employee={null} onClose={() => setAdding(false)} /> : null}
    </Page>
  );
}

type OrgState = "working" | "you" | "others" | "available";
const ORG_DOT: Record<OrgState, string> = {
  working: "var(--color-text-accent)",
  you: "var(--color-text-warning)",
  others: "var(--color-text-secondary)",
  available: "var(--precision-line2)",
};
const ORG_LABEL: Record<OrgState, string> = {
  working: "Working",
  you: "Waiting on you",
  others: "Waiting on others",
  available: "Available",
};

/** From its open tasks: whoever must act next. Actually working goes first. */
function orgState(e: EmployeeSummary, tasks: TaskView[]): OrgState {
  if (e.working && e.status !== "paused") return "working";
  const mine = tasks.filter((t) => t.employee_id === e.id);
  if (mine.some((t) => t.pending_approvals > 0 || (t.status === "waiting" && t.waiting_reason === "input")))
    return "you";
  return mine.some((t) => t.status === "waiting" || (t.status === "queued" && t.waiting_reason))
    ? "others"
    : "available";
}

function OrgCard({ node, tasks }: { node: OrgNode; tasks: TaskView[] }) {
  const e = node.employee;
  const state = orgState(e, tasks);
  const working = state === "working";
  return (
    <Link
      to="/employees/$id"
      params={{ id: e.id }}
      title={`${e.name} · ${position(e)}`}
      {...stylex.props(styles.orgCard, working && styles.orgWorking, e.status === "paused" && styles.orgPaused)}
    >
      <Mark letter={initial(e.name)} color={markColor(e.id)} size={26} radius={7} />
      <span {...stylex.props(styles.orgText)}>
        <span {...stylex.props(styles.orgName)}>{e.name}</span>
        <span {...stylex.props(styles.orgRole)}>{e.status === "paused" ? "Paused" : position(e)}</span>
      </span>
      <span
        role="img"
        aria-label={ORG_LABEL[state]}
        {...stylex.props(styles.orgDot)}
        style={{ backgroundColor: ORG_DOT[state] }}
      />
    </Link>
  );
}

/** A card with its reports hanging from a bar; the bar spans from the first child's middle to the last's. */
function OrgBranch({ node, tasks }: { node: OrgNode; tasks: TaskView[] }) {
  return (
    <li {...stylex.props(styles.branch)}>
      <OrgCard node={node} tasks={tasks} />
      {node.children.length > 0 ? (
        <>
          <span {...stylex.props(styles.stem)} />
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
                  <OrgBranch node={c} tasks={tasks} />
                </ul>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </li>
  );
}

export function OrgChartScreen() {
  const employees = useEmployees();
  const proposals = useProposals();
  const tasks = useOpenTasks();
  const open = tasks.data ?? [];
  const all = employees.data ?? [];
  const tree = useMemo(() => orgTree(all), [all]);
  const pending = (proposals.data ?? []).filter((p) => p.status === "pending");
  const dot = (color: string, label: string) => (
    <span key={label} {...stylex.props(styles.legendItem)}>
      <span {...stylex.props(styles.orgDot)} style={{ backgroundColor: color }} />
      {label}
    </span>
  );
  return (
    <Page>
      <PageHead title="Employees" tabs={<EmployeesTabs />}>
        <span {...stylex.props(styles.quiet)}>
          Who reports to whom across the company. Change it with Edit (F2) on an employee: Reports to.
        </span>
      </PageHead>
      <div {...stylex.props(styles.chart)}>
        {pending.length > 0 ? (
          <div {...stylex.props(styles.proposals)}>
            {pending.map((p) => (
              <ProposalCard key={p.id} proposal={p} all={all} />
            ))}
          </div>
        ) : null}
        {employees.data && all.length === 0 ? <p {...stylex.props(styles.chartEmpty)}>No employees yet.</p> : null}
        <ul {...stylex.props(styles.roots)}>
          {tree.map((n) => (
            <OrgBranch key={n.employee.id} node={n} tasks={open} />
          ))}
        </ul>
      </div>
      <div {...stylex.props(styles.legend)}>
        {(["working", "you", "others", "available"] as const).map((k) => dot(ORG_DOT[k], ORG_LABEL[k]))}
      </div>
    </Page>
  );
}
