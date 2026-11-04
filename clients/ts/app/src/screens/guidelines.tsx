import { expectBody, ProtocolError } from "@aurelius/client";
import type { GuidelineProposalView, GuidelineTarget, GuidelineView, StarterTemplate } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useArchivedGuidelines,
  useEffectiveSets,
  useEmployees,
  useGuideline,
  useGuidelineAffected,
  useGuidelineProposals,
  useGuidelines,
  useGuidelineVersions,
  useProjects,
} from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { Dialog } from "../ui/overlay";
import { toast } from "../ui/toast";
import {
  ABOUT,
  affectedLine,
  appliesTo,
  approveProblem,
  BANNER,
  bannerDue,
  blockSentence,
  blockWhere,
  changes,
  chars,
  contexts,
  count,
  counterLine,
  counterNote,
  createdLine,
  EMPTY_ABOUT,
  EMPTY_PROPOSE,
  editorMessage,
  employeeBlock,
  failureText,
  GROUP_NOTE,
  groups,
  largestWith,
  level,
  lineDiff,
  linkChanges,
  type Names,
  paneHeight,
  projectBlock,
  proposalHeadline,
  READ_ONLY,
  RUN_LIMIT,
  type Sets,
  SIDE_MIN_W,
  type Sort,
  safeMarkdown,
  saveProblem,
  shown,
  sourceLabel,
  stale,
  stored,
  TEXT_LIMIT,
  targetKey,
  tokens,
  tri,
  wanted,
} from "./guideline-logic";
import { AREA, Confirm, Empty, EYEBROW, FIELD, failureOf, KnowledgeTabs, PageHead, SMALL, shortDay } from "./org-kit";

function useNames(): Names {
  const employees = useEmployees();
  const projects = useProjects();
  return useMemo(
    () => ({ employees: employees.data ?? [], projects: projects.data ?? [] }),
    [employees.data, projects.data],
  );
}

function useSets(list: GuidelineView[], names: Names) {
  const where = useMemo(() => contexts(list, names.projects), [list, names.projects]);
  const pairs = useMemo(() => wanted(names.employees, where), [names.employees, where]);
  const sets = useEffectiveSets(pairs);
  return { sets: useMemo<Sets>(() => sets.data ?? new Map(), [sets.data]), where };
}

const canEdit = (user: unknown) => user !== null;

function writeFailure(e: unknown, names: Names): string {
  if (e instanceof ProtocolError) {
    return failureText({ code: e.code, message: e.message, tooLarge: e.tooLarge, matchedItems: e.matchedItems }, names);
  }
  return failureOf(e);
}

function StarterDialog({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [templates, setTemplates] = useState<StarterTemplate[] | null>(null);
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    connection
      .call("guideline.starter_set")
      .then((body) => {
        const list = expectBody(body, "starter_set").value.templates;
        setTemplates(list);
        setTicked(new Set(list.filter((t) => !t.in_library).map((t) => t.key)));
      })
      .catch((e: unknown) => setError(failureOf(e)));
  }, [connection]);
  const add = async () => {
    setBusy(true);
    try {
      await connection.write("guideline.add_starter_set", { keys: [...ticked] }, { retry: false });
      await client.invalidateQueries({ queryKey: ["guidelines"] });
      toast(`Added ${ticked.size} guideline${ticked.size === 1 ? "" : "s"}. Link them to employees to use them.`);
      onClose();
    } catch (e) {
      setError(failureOf(e));
      setBusy(false);
    }
  };
  return (
    <Dialog open onClose={onClose} title="Add starter set" width={520}>
      <div className="flex max-h-[60vh] flex-col gap-3 overflow-y-auto p-4">
        <p className="m-0 text-[15px] font-semibold">Add starter set</p>
        <p className="m-0 text-text2">
          Templates to start from. They become ordinary guidelines without links: link them to use them.
        </p>
        {templates === null && !error ? <p className="m-0 text-text3">Loading…</p> : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
        {templates?.map((t) => (
          <label key={t.key} className={`flex items-start gap-2 ${t.in_library ? "text-text3" : "cursor-pointer"}`}>
            <input
              type="checkbox"
              className="mt-[3px] accent-(--acc-text)"
              disabled={t.in_library}
              checked={ticked.has(t.key)}
              onChange={(e) => {
                const next = new Set(ticked);
                if (e.target.checked) next.add(t.key);
                else next.delete(t.key);
                setTicked(next);
              }}
            />
            <span>
              {t.title}
              <span className="block text-[12px] text-text3">
                {count(t.chars)} characters · for {t.level === "project" ? "a project" : "employees"}
                {t.in_library ? " · already in the library" : ""}
              </span>
            </span>
          </label>
        ))}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy || ticked.size === 0} onClick={() => void add()}>
          {busy ? "Adding…" : `Add ${ticked.size}`}
        </Button>
      </div>
    </Dialog>
  );
}

const GRID = "grid grid-cols-[minmax(220px,1.3fr)_minmax(220px,1.4fr)_60px_70px_150px] items-center gap-3 px-[18px]";

export function GuidelinesScreen() {
  const { user, features } = useLinkState();
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const platform = usePlatform();
  const names = useNames();
  const [archived, setArchived] = useState(false);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("title");
  const [starter, setStarter] = useState(false);
  const [done, setDone] = useState<string[]>(() => {
    try {
      return JSON.parse(platform.storage.get("guidelines_banner_done") ?? "[]") as string[];
    } catch {
      return [];
    }
  });
  const search = useRef<HTMLInputElement>(null);
  const active = useGuidelines();
  const gone = useArchivedGuidelines(archived);
  const proposals = useGuidelineProposals();
  const source = archived ? gone : active;
  const list = useMemo(() => shown(source.data ?? [], query, sort, names), [source.data, query, sort, names]);
  const writable = canEdit(user);
  const open = (id: string) => void navigate({ to: "/knowledge/guidelines/$id", params: { id } });
  useAction("NewGuideline", () => open("new"), writable);
  useAction("FindGuideline", () => {
    setArchived(false);
    search.current?.focus();
  });

  if (!features.includes("guidelines")) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />} />
        <p className="m-0 p-6 text-text2">This server does not have guidelines yet. Update the server.</p>
      </div>
    );
  }

  const pending = (g: GuidelineView) => (proposals.data ?? []).filter((p) => p.guideline_id === g.id);
  const showBanner = !archived && writable && bannerDue(active.data ?? [], user?.id ?? null, done);
  const head = (key: Sort, label: string, right = false) => (
    <button
      type="button"
      onClick={() => setSort(key)}
      className={`cursor-pointer border-0 bg-transparent p-0 text-[12px] text-text3 hover:text-text ${right ? "text-right" : "text-left"}`}
    >
      {label}
      {sort === key ? " ▾" : ""}
    </button>
  );
  const restore = async (g: GuidelineView) => {
    try {
      await connection.write("guideline.restore", { guideline_id: g.id }, { retry: false });
      await client.invalidateQueries({ queryKey: ["guidelines"] });
      toast(`Restored “${g.title}”. Link it again to use it.`);
    } catch (e) {
      toast(writeFailure(e, names));
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
        <input
          ref={search}
          aria-label="Find a guideline"
          placeholder="Find… (@Forge, ▸project)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className={`${FIELD} !h-7 !w-[220px]`}
        />
        <Button kind="ghost" aria-pressed={archived} className={SMALL} onClick={() => setArchived(!archived)}>
          Archived
        </Button>
        {writable ? (
          <Button kind="primary" className={SMALL} onClick={() => open("new")}>
            New guideline
          </Button>
        ) : null}
      </PageHead>
      {showBanner ? (
        <div className="mx-[18px] mt-[14px] flex items-center gap-[10px] rounded-[8px] border border-acc-line bg-acc-soft px-3 py-[10px]">
          <span className="size-[7px] flex-none rounded-full bg-acc-text" />
          <span className="min-w-0 flex-1">{BANNER}</span>
          <Button
            className="h-[26px] px-2 text-[12px]"
            onClick={() => {
              const next = [...done, user?.id ?? ""];
              setDone(next);
              platform.storage.set("guidelines_banner_done", JSON.stringify(next));
            }}
          >
            Got it
          </Button>
        </div>
      ) : null}
      {!writable ? <p className="m-0 px-[18px] pt-3 text-text2">{READ_ONLY}</p> : null}
      <div className="min-h-0 flex-1 overflow-auto">
        {source.isPending ? (
          <p className="m-0 p-6 text-text3">Loading…</p>
        ) : (source.data ?? []).length === 0 ? (
          archived ? (
            <p className="m-0 p-6 text-text3">Nothing is archived.</p>
          ) : (
            <Empty title="No guidelines yet">
              <p className="m-0 leading-[1.6] text-text2">{EMPTY_ABOUT}</p>
              {writable ? (
                <div className="flex gap-2">
                  <Button kind="primary" onClick={() => open("new")}>
                    + New guideline
                  </Button>
                  <Button onClick={() => setStarter(true)}>Add starter set…</Button>
                </div>
              ) : null}
              <p className="m-0 text-[12px] text-text3">{EMPTY_PROPOSE}</p>
            </Empty>
          )
        ) : (
          <div className="min-w-[760px]">
            <div className={`${GRID} h-8 border-b border-line2`}>
              {head("title", "Title")}
              <span className="text-[12px] text-text3">Applies to</span>
              {head("size", "Size", true)}
              <span className="text-[12px] text-text3">Version</span>
              {head("updated", "Updated")}
            </div>
            {list.length === 0 ? <p className="m-0 p-6 text-text3">No guideline matches.</p> : null}
            {list.map((g) => {
              const applies = appliesTo(g.links, names);
              const waiting = Math.max(g.open_proposals ?? 0, pending(g).length);
              return (
                <div key={g.id} className={`${GRID} min-h-[42px] border-b border-line2 hover:bg-sel`}>
                  <Link
                    to="/knowledge/guidelines/$id"
                    params={{ id: g.id }}
                    className="min-w-0 truncate font-medium text-text no-underline"
                  >
                    {g.title}
                    {waiting > 0 ? (
                      <span
                        title={waiting === 1 ? "1 proposal waits for you" : `${waiting} proposals wait for you`}
                        className="ml-2 text-acc-text"
                      >
                        ◆{waiting}
                      </span>
                    ) : null}
                  </Link>
                  <span
                    title={applies.all}
                    className={`truncate text-[12px] ${g.links.length === 0 ? "text-text3" : "text-text2"}`}
                  >
                    {applies.shown}
                  </span>
                  <span className="text-right font-mono text-[12px] text-text3">{count(g.chars)}</span>
                  <span className="text-[12px] text-text3">v{g.version}</span>
                  {archived && writable ? (
                    <Button className="h-[26px] justify-self-start px-2 text-[12px]" onClick={() => void restore(g)}>
                      Restore
                    </Button>
                  ) : (
                    <span className="truncate text-[12px] text-text3">
                      {g.updated_by ? (g.updated_by.id === user?.id ? "You" : g.updated_by.username) : "Starter set"} ·{" "}
                      {shortDay(g.updated_at_ms)}
                    </span>
                  )}
                </div>
              );
            })}
            <p className="m-0 px-[18px] py-3 text-[12px] text-text3">
              {archived
                ? `${list.length} archived · no run gets these; Restore puts one back`
                : `${list.length} guideline${list.length === 1 ? "" : "s"} · ${ABOUT.charAt(0).toLowerCase()}${ABOUT.slice(1, -1)}. Size in characters (max ${count(TEXT_LIMIT)} each)`}
            </p>
          </div>
        )}
      </div>
      {starter ? <StarterDialog onClose={() => setStarter(false)} /> : null}
    </div>
  );
}

function LinksDialog({
  title,
  guideline,
  size,
  initial,
  list,
  saveLabel,
  onSave,
  onClose,
}: {
  title: string;
  guideline: string | null;
  size: number;
  initial: GuidelineTarget[];
  list: GuidelineView[];
  saveLabel: string;
  onSave: (links: GuidelineTarget[], before: Set<string>, after: Set<string>) => Promise<void>;
  onClose: () => void;
}) {
  const names = useNames();
  const { sets, where } = useSets(list, names);
  const before = useMemo(() => new Set(initial.map(targetKey)), [initial]);
  const [ticked, setTicked] = useState<Set<string>>(() => new Set(before));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const keep = initial.filter((t) => t.kind === "employee").map((t) => t.id);
  const grouped = useMemo(() => groups(names.employees, keep), [names.employees, keep]);
  const asTargets = (set: Set<string>) =>
    [...set].map((k): GuidelineTarget => {
      const at = k.indexOf(":");
      return { kind: k.slice(0, at) as GuidelineTarget["kind"], id: k.slice(at + 1) };
    });
  const largest = largestWith(sets, guideline, size, asTargets(ticked));
  const toggle = (key: string, on: boolean) => {
    const next = new Set(ticked);
    if (on) next.add(key);
    else next.delete(key);
    setTicked(next);
  };
  const blockOf = (employee: string) => employeeBlock(sets, employee, where, { id: guideline, chars: size });
  const blockedEmployee = (id: string) => {
    const block = blockOf(id);
    return !before.has(`employee:${id}`) && !ticked.has(`employee:${id}`) && block && block.chars > RUN_LIMIT
      ? block
      : null;
  };
  return (
    <Dialog open onClose={onClose} title={title} width={640}>
      <div className="flex max-h-[64vh] flex-col gap-3 overflow-y-auto p-4">
        <p className="m-0 text-[15px] font-semibold">{title}</p>
        <p className={EYEBROW}>EMPLOYEES</p>
        {grouped.map((g) => {
          const state = tri(g.members, ticked);
          return (
            <fieldset key={g.label} className="m-0 flex flex-col gap-1 border-0 p-0">
              <label className="flex cursor-pointer items-center gap-2 font-medium">
                <input
                  type="checkbox"
                  className="accent-(--acc-text)"
                  ref={(el) => {
                    if (el) el.indeterminate = state.state === "some";
                  }}
                  checked={state.state === "all"}
                  onChange={() => {
                    const next = new Set(ticked);
                    for (const m of g.members) {
                      const key = `employee:${m}`;
                      if (state.state === "all") next.delete(key);
                      else if (!blockedEmployee(m)) next.add(key);
                    }
                    setTicked(next);
                  }}
                />
                {g.label}
                <span className="text-[12px] font-normal text-text3">
                  {state.ticked} of {state.of}
                </span>
              </label>
              {g.members.map((id) => {
                const e = names.employees.find((x) => x.id === id);
                const blocked = blockedEmployee(id);
                return (
                  <label
                    key={id}
                    title={blocked ? blockSentence(blocked, names) : undefined}
                    className={`ml-6 flex items-center gap-2 ${blocked ? "text-text3" : "cursor-pointer"}`}
                  >
                    <input
                      type="checkbox"
                      className="accent-(--acc-text)"
                      disabled={blocked !== null}
                      checked={ticked.has(`employee:${id}`)}
                      onChange={(ev) => toggle(`employee:${id}`, ev.target.checked)}
                    />
                    {e?.name}
                    {e?.status === "paused" ? " (paused)" : e?.status === "archived" ? " (archived)" : ""}
                  </label>
                );
              })}
            </fieldset>
          );
        })}
        <p className="m-0 text-[12px] text-text3">{GROUP_NOTE}</p>
        <p className={EYEBROW}>PROJECTS</p>
        {names.projects.map((p) => {
          const block = projectBlock(sets, p.id, where, names.employees, guideline, size);
          const key = `project:${p.id}`;
          const blocked = !before.has(key) && !ticked.has(key) && block && block.chars > RUN_LIMIT ? block : null;
          return (
            <label
              key={p.id}
              title={blocked ? blockSentence(blocked, names) : undefined}
              className={`flex items-center gap-2 ${blocked ? "text-text3" : "cursor-pointer"}`}
            >
              <input
                type="checkbox"
                className="accent-(--acc-text)"
                disabled={blocked !== null}
                checked={ticked.has(key)}
                onChange={(ev) => toggle(key, ev.target.checked)}
              />
              {p.name}
              <span className="text-[12px] text-text3">everyone working in this project</span>
            </label>
          );
        })}
        {largest ? (
          <p className="m-0 text-[12px] text-text3">
            largest: {blockWhere(largest, names)} · {count(largest.chars)}
          </p>
        ) : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button
          kind="primary"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            setError(null);
            onSave(asTargets(ticked), before, ticked)
              .then(onClose)
              .catch((e: unknown) => {
                setError(writeFailure(e, names));
                setBusy(false);
              });
          }}
        >
          {busy ? "Saving…" : saveLabel}
        </Button>
      </div>
    </Dialog>
  );
}

/** Saves link changes one by one and says how far it got. */
async function saveLinks(
  write: (
    method: "guideline.link" | "guideline.unlink",
    params: { guideline_id: string; target: GuidelineTarget },
  ) => Promise<unknown>,
  guideline: string,
  before: Set<string>,
  after: Set<string>,
) {
  const list = linkChanges(before, after);
  let done = 0;
  for (const c of list) {
    try {
      await write(c.link ? "guideline.link" : "guideline.unlink", { guideline_id: guideline, target: c.target });
      done++;
    } catch (e) {
      if (done > 0) throw new Error(`${done} of ${list.length} changes were saved, then: ${failureOf(e)}`);
      throw e;
    }
  }
  if (done > 0) toast(`Saved ${done} link change${done === 1 ? "" : "s"}. It applies from the next run.`);
}

function History({
  id,
  title,
  text,
  current,
  names,
  canRestore,
  onRestore,
}: {
  id: string;
  title: string;
  text: string;
  current: number;
  names: Names;
  canRestore: boolean;
  onRestore: (version: number) => void;
}) {
  const versions = useGuidelineVersions(id, true);
  const proposals = useGuidelineProposals();
  const [pick, setPick] = useState<number | null>(null);
  const picked = versions.data?.find((v) => v.version === pick);
  const diff = useMemo(() => (picked ? lineDiff(picked.text, text) : []), [picked, text]);
  return (
    <aside
      aria-label="History"
      className="enter flex w-[380px] flex-none flex-col gap-3 overflow-y-auto border-l border-line bg-sunk p-[18px]"
    >
      <p className={EYEBROW}>HISTORY · {title.toUpperCase()}</p>
      {versions.isPending ? <p className="m-0 text-text3">Loading…</p> : null}
      <div className="flex max-h-[320px] flex-col gap-2 overflow-y-auto">
        {versions.data?.map((v) => (
          <div
            key={v.version}
            className={`flex flex-col gap-1 rounded-[8px] border bg-panel px-3 py-[10px] ${v.version === current ? "border-acc-line" : "border-line"}`}
          >
            <button
              type="button"
              aria-pressed={pick === v.version}
              onClick={() => setPick(pick === v.version ? null : v.version)}
              className="cursor-pointer border-0 bg-transparent p-0 text-left text-text"
            >
              Version {v.version}
              {v.version === current ? " · current" : ""} · {v.by?.username ?? "Aurelius"} · {shortDay(v.created_at_ms)}
              <span className="block text-[12px] text-text3">{sourceLabel(v, proposals.data ?? [], names)}</span>
            </button>
            {canRestore && v.version !== current ? (
              <Button className="h-6 self-start px-2 text-[12px]" onClick={() => onRestore(v.version)}>
                Restore
              </Button>
            ) : null}
          </div>
        ))}
      </div>
      {picked ? (
        <div className="flex flex-col gap-1">
          <p className={EYEBROW}>Version {picked.version} → what the editor holds</p>
          <Diff lines={diff} />
        </div>
      ) : null}
      <p className="m-0 text-[12px] text-text3">Links have no versions: only the title and the text do.</p>
    </aside>
  );
}

function Diff({ lines }: { lines: ReturnType<typeof lineDiff> }) {
  return (
    <pre className="m-0 max-h-[320px] overflow-auto rounded-[6px] border border-line bg-panel p-[10px] font-mono text-[12px] leading-[17px] whitespace-pre-wrap">
      {lines.map((l, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: a diff is drawn whole.
          key={i}
          className={l.mark === "+" ? "bg-sel text-acc-text" : l.mark === "-" ? "bg-sel text-red" : "text-text2"}
        >
          {l.mark} {l.line}
        </div>
      ))}
    </pre>
  );
}

function RejectDialog({
  proposal,
  who,
  onClose,
}: {
  proposal: GuidelineProposalView;
  who: string;
  onClose: () => void;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const [note, setNote] = useState("");
  const reject = async () => {
    try {
      await connection.write(
        "guideline.decide_proposal",
        { proposal_id: proposal.id, approve: false, ...(note.trim() ? { note: note.trim() } : {}) },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["guideline_proposals"] });
      toast("Rejected. The employee gets your decision and note.");
      onClose();
    } catch (e) {
      toast(failureOf(e));
    }
  };
  return (
    <Dialog open onClose={onClose} title="Reject this proposal?" width={480}>
      <div className="flex flex-col gap-2 p-4">
        <p className="m-0 text-[15px] font-semibold">Reject this proposal?</p>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Note for {who}
          <textarea
            data-autofocus
            rows={3}
            className={AREA}
            placeholder="Optional: why, for the employee who proposed it"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </label>
        <p className="m-0 text-[12px] text-text3">{who} gets your decision and the note.</p>
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="danger" onClick={() => void reject()}>
          Reject
        </Button>
      </div>
    </Dialog>
  );
}

export function GuidelineEditor({ id, proposalId }: { id: string; proposalId: string | null }) {
  useKeyContext("AureliusDialog");
  const isNew = id === "new";
  const { user } = useLinkState();
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const names = useNames();
  const all = useGuidelines();
  const detail = useGuideline(isNew ? null : id);
  const proposals = useGuidelineProposals();
  const proposal = proposalId ? (proposals.data?.find((p) => p.id === proposalId) ?? null) : null;
  const affected = useGuidelineAffected(isNew ? null : id);
  const current = detail.data?.guideline ?? null;
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [base, setBase] = useState<{ title: string; text: string; version: number } | null>(null);
  const [links, setLinks] = useState<GuidelineTarget[] | null>(null);
  const [mode, setMode] = useState<"write" | "preview" | "side">(() =>
    window.innerWidth >= SIDE_MIN_W ? "side" : "write",
  );
  const [history, setHistory] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [conflict, setConflict] = useState<number | null>(null);
  const [compare, setCompare] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editLinks, setEditLinks] = useState(false);
  const [confirm, setConfirm] = useState<"leave" | "archive" | { restore: number } | null>(null);
  const [rejecting, setRejecting] = useState(false);
  const [height, setHeight] = useState(() => paneHeight(window.innerHeight));
  useEffect(() => {
    const onResize = () => setHeight(paneHeight(window.innerHeight));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  // The text is taken once: a change elsewhere must not overwrite what is being typed.
  useEffect(() => {
    if (base) return;
    // A proposal's text goes first, so nothing is taken before it has arrived.
    if (proposalId && !proposal) return;
    if (proposal?.guideline_id && !detail.data) return;
    if (proposal) {
      setTitle(proposal.title);
      setText(proposal.text);
      setLinks(proposal.suggested_links ?? []);
      setBase({
        title: detail.data?.version.title ?? "",
        text: detail.data?.version.text ?? "",
        version: proposal.base_version ?? 0,
      });
      return;
    }
    if (isNew) {
      setBase({ title: "", text: "", version: 0 });
      setLinks([]);
    } else if (detail.data) {
      setTitle(detail.data.version.title);
      setText(detail.data.version.text);
      setBase({
        title: detail.data.version.title,
        text: detail.data.version.text,
        version: detail.data.guideline.version,
      });
      setLinks(detail.data.guideline.links);
    }
  }, [base, isNew, detail.data, proposal, proposalId]);

  const { sets } = useSets(all.data ?? [], names);
  const n = chars(text);
  const linkList = links ?? [];
  const largest = useMemo(() => largestWith(sets, isNew ? null : id, n, linkList), [sets, isNew, id, n, linkList]);
  const problem = saveProblem(title, text, largest, names);
  const message = editorMessage(failure, problem, title, n);
  const approveOff = proposal ? approveProblem({ ...proposal, text }, current, linkList, sets, names) : null;
  const dirty =
    base !== null &&
    changes(
      { title: proposal ? proposal.title : base.title, text: proposal ? proposal.text : base.text },
      { title, text },
    ) !== null;
  const archived = current?.archived === true;
  const writable = canEdit(user) && !archived;
  const isStale = proposal ? stale(proposal, current) : null;
  const heading = proposal ? "Edit and approve" : isNew ? "New guideline" : (current?.title ?? "");
  const leave = () => void navigate({ to: "/knowledge/$tab", params: { tab: "guidelines" } });
  const tryLeave = () => (dirty ? setConfirm("leave") : leave());
  useAction("Back", tryLeave);

  const refresh = () => client.invalidateQueries({ queryKey: ["guidelines"] });
  const edit = (set: (v: string) => void) => (v: string) => {
    set(v);
    setFailure(null);
  };

  const save = async () => {
    if (busy || !writable || !base) return;
    if (problem) return setFailure(problem);
    if (proposal && approveOff) return setFailure(approveOff);
    setBusy(true);
    setFailure(null);
    try {
      if (proposal) {
        const stayed = { title: title.trim(), text: stored(text) };
        await connection.write(
          "guideline.decide_proposal",
          { proposal_id: proposal.id, approve: true, ...stayed, ...(proposal.guideline_id ? {} : { links: linkList }) },
          { retry: false, ...(proposal.base_version != null ? { expectedVersion: proposal.base_version } : {}) },
        );
        toast("Approved. It applies from the next run.");
      } else if (isNew) {
        await connection.write(
          "guideline.create",
          { title: title.trim(), text: stored(text), links: linkList },
          { retry: false },
        );
        toast(createdLine(title.trim(), linkList, names));
      } else {
        const changed = changes(base, { title, text });
        if (!changed) return leave();
        await connection.write(
          "guideline.update",
          { guideline_id: id, ...changed },
          { retry: false, expectedVersion: base.version },
        );
        toast(`Saved as version ${base.version + 1}. It applies from the next run.`);
      }
      await refresh();
      await client.invalidateQueries({ queryKey: ["guideline_proposals"] });
      leave();
    } catch (e) {
      if (e instanceof ProtocolError && e.code === "version_conflict") {
        // Nothing of the typed text goes: it stays beside the newer version.
        setConflict(base.version + 1);
        await client.invalidateQueries({ queryKey: ["guidelines", "detail", id] });
        return;
      }
      setFailure(writeFailure(e, names));
    } finally {
      setBusy(false);
    }
  };
  useAction("ConfirmPrimary", () => void save(), writable);

  const archive = async () => {
    try {
      await connection.write("guideline.archive", { guideline_id: id }, { retry: false });
      await refresh();
      toast(`Archived “${current?.title}”. No new run gets it; it is under Archived.`);
      leave();
    } catch (e) {
      setConfirm(null);
      setFailure(writeFailure(e, names));
    }
  };
  const restoreVersion = async (version: number) => {
    try {
      await connection.write("guideline.restore_version", { guideline_id: id, version }, { retry: false });
      await refresh();
      toast(`Restored version ${version}. It applies from the next run.`);
      leave();
    } catch (e) {
      setConfirm(null);
      setFailure(writeFailure(e, names));
    }
  };
  const discardMine = () => {
    const newer = detail.data?.version;
    if (!newer) return;
    setTitle(newer.title);
    setText(newer.text);
    setBase({ title: newer.title, text: newer.text, version: detail.data?.guideline.version ?? newer.version });
    setConflict(null);
    setFailure(null);
  };

  const safe = useMemo(() => safeMarkdown(text), [text]);
  const counter = level(n, TEXT_LIMIT);
  const blockLevel = largest ? level(largest.chars, RUN_LIMIT) : "calm";
  const tone = (l: string) => (l === "over" ? "bg-red" : l === "close" ? "bg-amber" : "bg-acc-text");
  const bar = (value: number, max: number, l: string) => (
    <span className="inline-block h-[3px] w-[120px] overflow-hidden rounded-[2px] bg-sel align-middle">
      <span className={`block h-full ${tone(l)}`} style={{ width: `${Math.min(100, (value / max) * 100)}%` }} />
    </span>
  );
  const whoProposed = proposal
    ? (names.employees.find((e) => e.id === proposal.proposed_by)?.name ?? "An employee")
    : "";

  if (!isNew && detail.error) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <PageHead
          title={
            <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} className="text-text2 no-underline">
              ‹ Guidelines
            </Link>
          }
        />
        <p role="alert" className="m-0 p-6 text-red">
          {detail.error.message}
        </p>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <PageHead
          title={
            <>
              <Link
                to="/knowledge/$tab"
                params={{ tab: "guidelines" }}
                onClick={(e) => {
                  if (dirty) {
                    e.preventDefault();
                    setConfirm("leave");
                  }
                }}
                className="text-text2 no-underline hover:text-text"
              >
                ‹ Guidelines
              </Link>
              <span className="truncate">· {heading}</span>
              {current ? (
                <span className="rounded-[4px] bg-sel px-[6px] text-[12px] text-text2">v{current.version}</span>
              ) : null}
            </>
          }
        >
          {!isNew && !proposal ? (
            <Button kind="ghost" aria-expanded={history} className={SMALL} onClick={() => setHistory(!history)}>
              History {history ? "◂" : "▸"}
            </Button>
          ) : null}
          {!isNew && !proposal && writable ? (
            <Button kind="ghost" className={SMALL} onClick={() => setConfirm("archive")}>
              Archive…
            </Button>
          ) : null}
          <Button className={SMALL} onClick={tryLeave}>
            Cancel
          </Button>
          {writable ? (
            <Button
              kind="primary"
              className={`${SMALL} ${problem || busy ? "opacity-60" : ""}`}
              onClick={() => void save()}
            >
              {busy ? "Saving…" : proposal ? "Approve" : "Save"}
            </Button>
          ) : null}
        </PageHead>
        <div className="flex min-h-0 flex-1 flex-col gap-[14px] overflow-y-auto px-[22px] py-5">
          {archived ? (
            <p className="m-0 text-text2">Archived: no run gets this. Restore it from the library to change it.</p>
          ) : null}
          {proposal ? (
            <div className="flex flex-col gap-1 rounded-[8px] border border-amber-line bg-amber-soft px-3 py-[10px]">
              <p className={EYEBROW}>GUIDELINE PROPOSED · WAITING FOR YOU</p>
              <p className="m-0 font-medium">{proposalHeadline(proposal, current, names)}</p>
              {proposal.reason ? <p className="m-0 text-text2">{proposal.reason}</p> : null}
              {isStale ? (
                <p role="alert" className="m-0 text-red">
                  “{current?.title}” changed since this was proposed (version {isStale.base} → {isStale.now}), so it
                  can't be approved. Reject it; {whoProposed} can propose again on the new text.
                </p>
              ) : approveOff ? (
                <p className="m-0 text-amber">{approveOff}</p>
              ) : null}
              <Button kind="danger" className={`${SMALL} self-start`} onClick={() => setRejecting(true)}>
                Reject
              </Button>
            </div>
          ) : null}
          {conflict !== null ? (
            <div
              role="alert"
              className="flex flex-col gap-2 rounded-[8px] border border-amber-line bg-amber-soft px-3 py-[10px]"
            >
              <p className="m-0 font-medium">
                Version {conflict} was saved while you were editing. Nothing of yours was saved.
              </p>
              <div className="flex gap-2">
                <Button className={SMALL} aria-expanded={compare} onClick={() => setCompare(!compare)}>
                  {compare ? "Hide compare" : "Compare"}
                </Button>
                <Button className={SMALL} onClick={discardMine}>
                  Discard mine
                </Button>
              </div>
              {compare && detail.data ? (
                <>
                  <p className={EYEBROW}>Version {detail.data.version.version} → yours</p>
                  <Diff lines={lineDiff(detail.data.version.text, text)} />
                </>
              ) : null}
            </div>
          ) : null}
          <label className="flex flex-col gap-1 text-[12px] text-text3">
            Title *
            <input
              data-autofocus
              readOnly={!writable}
              placeholder="Rust"
              className={`${FIELD} !h-[34px] text-[14px] font-medium`}
              value={title}
              onChange={(e) => edit(setTitle)(e.target.value)}
            />
          </label>
          <fieldset
            aria-label="View"
            className="m-0 flex gap-px self-start rounded-[6px] border border-line2 bg-sunk p-[2px]"
          >
            {(["write", "preview", "side"] as const).map((m) => (
              <button
                key={m}
                type="button"
                aria-pressed={mode === m}
                onClick={() => setMode(m)}
                className={`h-[22px] cursor-pointer rounded-[4px] border-0 px-2 text-[12px] font-medium ${mode === m ? "bg-sel text-text" : "bg-transparent text-text2"}`}
              >
                {m === "write" ? "Write" : m === "preview" ? "Preview" : "Side by side"}
              </button>
            ))}
          </fieldset>
          <div
            className={`grid gap-3 ${mode === "side" ? "grid-cols-2" : "grid-cols-1"}`}
            style={{ minHeight: height }}
          >
            {mode !== "preview" ? (
              <textarea
                aria-label="Text"
                readOnly={!writable}
                placeholder="The rules, in Markdown: ## headings and - lists work."
                style={{ height }}
                className="w-full resize-none rounded-[8px] border border-line bg-sunk p-[14px] font-mono text-[12px] leading-[1.7] text-text2 outline-none focus:border-acc-text focus-visible:outline-none"
                value={text}
                onChange={(e) => edit(setText)(e.target.value)}
              />
            ) : null}
            {mode !== "write" ? (
              <div
                style={{ height }}
                className="overflow-y-auto rounded-[8px] border border-line px-[18px] py-4 leading-[1.65] [&>*:first-child]:mt-0"
              >
                <Markdown text={safe} safe />
              </div>
            ) : null}
          </div>
          <div className="flex flex-wrap items-center gap-x-5 gap-y-1 text-[12px] text-text3">
            <span className="flex items-center gap-2">
              {bar(n, TEXT_LIMIT, counter)} {counterLine(n)} · {tokens(n)}
              {counterNote(n) ? (
                <span className={counter === "over" ? "text-red" : "text-amber"}>{counterNote(n)}</span>
              ) : null}
            </span>
            <span className="flex items-center gap-2">
              Largest block with this:{" "}
              {largest ? (
                <>
                  {bar(largest.chars, RUN_LIMIT, blockLevel)} {blockWhere(largest, names)} · {count(largest.chars)} /{" "}
                  {count(RUN_LIMIT)} · {tokens(largest.chars)} per run
                </>
              ) : (
                "no run gets it yet"
              )}
            </span>
          </div>
          {message ? (
            <p role="alert" className="m-0 text-red">
              {message}
            </p>
          ) : null}
          <div className="flex items-center gap-3 rounded-[8px] border border-line px-3 py-[10px]">
            <span className="text-[12px] text-text3">Applies to</span>
            <span title={appliesTo(linkList, names).all} className="min-w-0 flex-1 truncate">
              {appliesTo(linkList, names).shown}
            </span>
            {writable ? (
              <Button className={SMALL} onClick={() => setEditLinks(true)}>
                Links…
              </Button>
            ) : null}
          </div>
          <p className="m-0 text-[12px] text-text3">
            {affected.data
              ? `${affectedLine(affected.data)} Runs that already started keep version ${current?.version}.`
              : "Applies from the next run."}
          </p>
        </div>
      </div>
      {history && current ? (
        <History
          id={id}
          title={current.title}
          text={text}
          current={current.version}
          names={names}
          canRestore={writable}
          onRestore={(version) => setConfirm({ restore: version })}
        />
      ) : null}
      {editLinks ? (
        <LinksDialog
          title={`Who gets “${title.trim() || "this guideline"}”`}
          guideline={isNew ? null : id}
          size={n}
          initial={linkList}
          list={all.data ?? []}
          saveLabel={isNew || proposal ? "Done" : "Save"}
          onClose={() => setEditLinks(false)}
          onSave={async (next, before, after) => {
            if (!isNew && !proposal) {
              await saveLinks(
                (method, params) => connection.write(method, params, { retry: false }),
                id,
                before,
                after,
              );
              await refresh();
            }
            setLinks(next);
          }}
        />
      ) : null}
      {confirm === "leave" ? (
        <Confirm
          danger
          title="Discard your changes?"
          body="What you typed in this guideline is not saved."
          action="Discard"
          onClose={() => setConfirm(null)}
          onConfirm={leave}
        />
      ) : null}
      {confirm === "archive" ? (
        <Confirm
          danger
          title={`Archive “${current?.title}”?`}
          body={`No new run gets it; a run that already started keeps it. Its links go (${linkList.length ? `now ${appliesTo(linkList, names).shown}` : "it is not linked"}). Its versions stay, and Restore under Archived brings it back.`}
          action="Archive"
          onClose={() => setConfirm(null)}
          onConfirm={() => void archive()}
        />
      ) : null}
      {typeof confirm === "object" && confirm !== null ? (
        <Confirm
          title={`Restore version ${confirm.restore} of ${current?.title}?`}
          body={`This becomes version ${(current?.version ?? 0) + 1} and applies from the next run. No version is lost.`}
          action="Restore"
          onClose={() => setConfirm(null)}
          onConfirm={() => void restoreVersion(confirm.restore)}
        />
      ) : null}
      {rejecting && proposal ? (
        <RejectDialog
          proposal={proposal}
          who={whoProposed}
          onClose={() => {
            setRejecting(false);
            if (!(proposals.data ?? []).some((p) => p.id === proposal.id)) leave();
          }}
        />
      ) : null}
    </div>
  );
}
