import { ProtocolError } from "@aurelius/client";
import type { EmployeeSummary, ProposalView, TaskView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEffectiveSets, useEmployees, useOpenTasks, useProposals } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { toast } from "../ui/toast";
import { EmployeeDialog } from "./employee-dialog";
import {
  canPin,
  employeeGroups,
  matchesSearch,
  type OrgNode,
  orgTree,
  position,
  togglePin,
  workOf,
} from "./employee-logic";
import { lineDiff } from "./guideline-logic";
import { AREA, EmployeesTabs, EYEBROW, FIELD, failureOf, PageHead, SMALL } from "./org-kit";

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
    <article className="flex max-w-[720px] flex-col gap-2 rounded-[10px] border border-acc-line bg-panel px-[18px] py-4">
      <p className={EYEBROW}>{isInstructions ? "INSTRUCTIONS PROPOSED" : "COLLEAGUE PROPOSED"} · WAITING FOR YOU</p>
      <h2 className="m-0 text-[15px] font-semibold">
        {isInstructions
          ? self
            ? `${by} proposes new instructions for itself`
            : `${by} proposes new instructions for ${target}`
          : `${by} proposes ${proposal.name}`}
      </h2>
      {isInstructions ? null : (
        <p className="m-0 text-text2">
          {[proposal.title, proposal.role, proposal.reports_to_name ? `reports to ${proposal.reports_to_name}` : null]
            .filter(Boolean)
            .join(" · ")}
        </p>
      )}
      {proposal.reason ? (
        <>
          <p className={EYEBROW}>WHY</p>
          <p className="m-0 leading-[1.5] text-text2">{proposal.reason}</p>
        </>
      ) : null}
      {isInstructions ? (
        <>
          <p className={EYEBROW}>CHANGE · NOW → PROPOSED</p>
          <pre className="m-0 max-h-[240px] overflow-auto rounded-[6px] border border-line bg-sunk p-[10px] font-mono text-[12px] leading-[17px] whitespace-pre-wrap">
            {lineDiff(proposal.current_instructions ?? "", proposal.instructions).map((l, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: a diff is drawn whole.
              <div key={i} className={l.mark === "+" ? "text-acc-text" : l.mark === "-" ? "text-red" : "text-text2"}>
                {l.mark} {l.line}
              </div>
            ))}
          </pre>
        </>
      ) : (
        <>
          <p className={EYEBROW}>INSTRUCTIONS</p>
          <p className="m-0 line-clamp-4 leading-[1.5] whitespace-pre-wrap text-text2">{proposal.instructions}</p>
        </>
      )}
      {stale ? (
        <p role="alert" className="m-0 text-red">
          {target}'s instructions changed since this was proposed (version {proposal.base_version} →{" "}
          {proposal.current_version}), so it can't be approved. Decline it; the proposer can propose again on the new
          text.
        </p>
      ) : null}
      {problem ? (
        <p role="alert" className="m-0 text-red">
          {problem}
        </p>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button className={SMALL} disabled={busy} onClick={() => void decide(false)}>
          Decline
        </Button>
        {isInstructions && !stale ? (
          <Button className={SMALL} disabled={busy} onClick={() => setEditing(true)}>
            Edit and approve…
          </Button>
        ) : null}
        {stale ? null : (
          <Button kind="primary" className={SMALL} disabled={busy} onClick={() => void decide(true)}>
            Approve
          </Button>
        )}
      </div>
      {editing ? (
        <Dialog open onClose={() => setEditing(false)} title="Edit and approve instructions" width={620}>
          <div className="flex flex-col gap-2 p-4">
            <p className="m-0 text-[15px] font-semibold">Edit and approve instructions</p>
            <label className="flex flex-col gap-1 text-[12px] text-text3">
              Instructions
              <textarea
                data-autofocus
                rows={12}
                className={AREA}
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
            </label>
            <p className="m-0 text-[12px] text-text3">
              This text replaces the employee's instructions as a new version; the proposer is told it was edited.
            </p>
            {text.trim() === "" ? (
              <p className="m-0 text-amber">Write the instructions to approve, or decline the proposal instead.</p>
            ) : null}
          </div>
          <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
            <Button onClick={() => setEditing(false)}>Cancel</Button>
            <Button kind="primary" disabled={busy || text.trim() === ""} onClick={() => void decide(true, text)}>
              Approve these
            </Button>
          </div>
        </Dialog>
      ) : null}
    </article>
  );
}

export function employeeState(e: EmployeeSummary): { label: string; color: string; dot: string } {
  if (e.status === "paused") return { label: "Paused", color: "text-text3", dot: "bg-text3" };
  if (e.working) return { label: "Working", color: "text-acc-text", dot: "bg-acc-text" };
  return { label: "Available", color: "text-text2", dot: "bg-text3" };
}

const GRID = "grid grid-cols-[minmax(200px,1.4fr)_150px_minmax(140px,1fr)_90px] items-center gap-3 px-[18px]";

export function EmployeesScreen() {
  const { hostName } = useLinkState();
  const navigate = useNavigate();
  const employees = useEmployees();
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
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead title="Employees" tabs={<EmployeesTabs />}>
        <input
          aria-label="Search employees"
          placeholder="Search employees by name, role or title"
          className={`${FIELD} !h-7 !w-[260px]`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Button kind="primary" className={SMALL} onClick={() => setAdding(true)}>
          Add employee
        </Button>
      </PageHead>
      <div className="min-h-0 flex-1 overflow-auto">
        {employees.data && all.length === 0 ? <p className="m-0 p-6 text-text3">No employees yet.</p> : null}
        {all.length > 0 && flat.length === 0 ? <p className="m-0 p-6 text-text3">Nobody matches.</p> : null}
        {flat.length > 0 ? (
          <div className="min-w-[560px]">
            <div className={`${GRID} h-8 border-b border-line2 text-[12px] text-text3`}>
              <span>Name</span>
              <span>Status</span>
              <span>Working on</span>
              <span>Guidelines</span>
            </div>
            {groups.map((g) => (
              <section key={g.label} aria-label={g.label}>
                <p className="m-0 flex h-[30px] items-center border-b border-line2 bg-sunk px-[18px] text-[12px] font-medium text-text2">
                  {g.label} · {g.list.length}
                </p>
                {g.list.map((e) => {
                  const s = employeeState(e);
                  const work = workOf(e.agents);
                  const set = sets.data?.get(`${e.id}|`);
                  const pinned = pins.includes(e.id);
                  return (
                    // biome-ignore lint/a11y/noStaticElementInteractions: the name is the link; the row only selects.
                    <div
                      key={e.id}
                      onClick={() => setSelected(e.id)}
                      onKeyDown={undefined}
                      className={`${GRID} group min-h-[42px] border-b border-line2 hover:bg-sel ${selected === e.id ? "bg-sel shadow-[inset_2px_0_0_var(--acc-text)]" : ""}`}
                    >
                      <span className="flex min-w-0 items-center gap-[10px]">
                        <Mark letter={initial(e.name)} color={markColor(e.id)} size={22} radius={6} />
                        <Link
                          to="/employees/$id"
                          params={{ id: e.id }}
                          className="min-w-0 truncate font-medium text-text no-underline hover:underline"
                        >
                          {e.name}
                        </Link>
                        <span className="min-w-0 truncate text-[12px] text-text3">{position(e)}</span>
                        <button
                          type="button"
                          aria-label={pinned ? `Unpin ${e.name}` : `Pin ${e.name}`}
                          aria-pressed={pinned}
                          onClick={(ev) => {
                            ev.stopPropagation();
                            if (!canPin(pins, e.id)) return toast("Only 5 employees can be pinned. Unpin one first.");
                            setPins(togglePin(pins, e.id));
                          }}
                          className={`ml-auto cursor-pointer border-0 bg-transparent p-0 text-[12px] hover:text-text focus-visible:opacity-100 group-hover:opacity-100 ${pinned ? "text-acc-text" : "text-text3 opacity-0"}`}
                        >
                          {pinned ? "★" : "☆"}
                        </button>
                      </span>
                      <span className={`flex items-center gap-2 text-[12px] ${s.color}`}>
                        <span className={`size-[6px] rounded-full ${s.dot}`} />
                        {s.label}
                      </span>
                      <span className="truncate text-[12px] text-text2">{work?.title ?? ""}</span>
                      <span className="text-[12px] text-text3">
                        {set ? `${set.guidelines.length} · ${set.total_chars}` : ""}
                      </span>
                    </div>
                  );
                })}
              </section>
            ))}
          </div>
        ) : null}
      </div>
      {adding ? <EmployeeDialog employee={null} onClose={() => setAdding(false)} /> : null}
    </div>
  );
}

type OrgState = "working" | "you" | "others" | "available";
const ORG_DOT: Record<OrgState, string> = {
  working: "bg-acc-text",
  you: "bg-amber",
  others: "bg-text2",
  available: "bg-line2",
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
      className={`flex w-[220px] items-center gap-[10px] rounded-[8px] border bg-panel px-3 py-[10px] text-text no-underline shadow-card hover:bg-sel ${working ? "border-acc-line" : "border-line"} ${e.status === "paused" ? "opacity-60" : ""}`}
    >
      <Mark letter={initial(e.name)} color={markColor(e.id)} size={26} radius={7} />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{e.name}</span>
        <span className="block truncate text-[12px] text-text3">{e.status === "paused" ? "Paused" : position(e)}</span>
      </span>
      <span
        role="img"
        aria-label={ORG_LABEL[state]}
        className={`size-[7px] flex-none rounded-full ${ORG_DOT[state]}`}
      />
    </Link>
  );
}

/** A card with its reports hanging from a bar; the bar spans from the first child's middle to the last's. */
function OrgBranch({ node, tasks }: { node: OrgNode; tasks: TaskView[] }) {
  return (
    <li className="flex list-none flex-col items-center">
      <OrgCard node={node} tasks={tasks} />
      {node.children.length > 0 ? (
        <>
          <span className="h-[22px] w-px bg-(--connector)" />
          <ul className="relative m-0 flex gap-4 p-0">
            {node.children.map((c, i) => (
              <li key={c.employee.id} className="relative flex list-none flex-col items-center">
                <span
                  className={`absolute top-0 h-px bg-(--connector) ${node.children.length === 1 ? "hidden" : i === 0 ? "right-[-8px] left-1/2" : i === node.children.length - 1 ? "right-1/2 left-[-8px]" : "right-[-8px] left-[-8px]"}`}
                />
                <span className="h-[22px] w-px bg-(--connector)" />
                <ul className="m-0 p-0">
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
    <span key={label} className="flex items-center gap-[6px]">
      <span className={`size-[7px] rounded-full ${color}`} />
      {label}
    </span>
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead title="Employees" tabs={<EmployeesTabs />}>
        <span className="text-[12px] text-text3">
          Who reports to whom across the company. Change it with Edit (F2) on an employee: Reports to.
        </span>
      </PageHead>
      <div className="dot-grid min-h-0 flex-1 overflow-auto px-6 py-9">
        {pending.length > 0 ? (
          <div className="mb-6 flex flex-col items-center gap-3">
            {pending.map((p) => (
              <ProposalCard key={p.id} proposal={p} all={all} />
            ))}
          </div>
        ) : null}
        {employees.data && all.length === 0 ? <p className="m-0 text-center text-text3">No employees yet.</p> : null}
        <ul className="m-0 flex w-max min-w-full justify-center gap-7 p-0">
          {tree.map((n) => (
            <OrgBranch key={n.employee.id} node={n} tasks={open} />
          ))}
        </ul>
      </div>
      <div className="flex flex-none flex-wrap gap-4 border-t border-line2 px-[18px] py-2 text-[12px] text-text3">
        {(["working", "you", "others", "available"] as const).map((k) => dot(ORG_DOT[k], ORG_LABEL[k]))}
      </div>
    </div>
  );
}
