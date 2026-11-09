import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import { expectBody, ProtocolError } from "@aurelius/client";
import type { GuidelineProposalView, GuidelineTarget, GuidelineView, StarterTemplate } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
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
import { Markdown } from "../ui/markdown";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
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
import {
  Confirm,
  Empty,
  Eyebrow,
  FormDialog,
  failureOf,
  Hint,
  KnowledgeTabs,
  Loading,
  Page,
  PageHead,
  Problem,
  shortDay,
} from "./org-kit";
import { guidelineReach, guidelinesIn } from "./workspace-logic";

const enter = stylex.keyframes({ from: { opacity: 0, transform: "translateY(4px)" } });

const styles = stylex.create({
  sub: { margin: 0, color: "var(--color-text-secondary)" },
  message: { margin: 0, padding: 24, color: "var(--color-text-secondary)" },
  empty: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  buttons: { display: "flex", gap: 8 },
  bannerSlot: { marginBlockStart: 14, marginInline: 18 },
  readOnly: { margin: 0, paddingInline: 18, paddingBlockStart: 12, color: "var(--color-text-secondary)" },
  scroll: { minHeight: 0, flex: 1, overflow: "auto" },
  table: { minWidth: 760 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  sizeColumn: { width: 60, minWidth: 60, maxWidth: 60, textAlign: "end" },
  versionColumn: { width: 70, minWidth: 70, maxWidth: 70 },
  updatedColumn: { width: 150, minWidth: 150, maxWidth: 150 },
  sort: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  row: { height: 42 },
  link: {
    display: "block",
    overflow: "hidden",
    fontWeight: 500,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-primary)",
    textDecoration: "none",
  },
  waiting: { marginInlineStart: 8, color: "var(--color-text-accent)" },
  applies: { display: "block", overflow: "hidden", fontSize: 12, whiteSpace: "nowrap", textOverflow: "ellipsis" },
  appliesOn: { color: "var(--color-text-secondary)" },
  dim: { color: "var(--precision-text3)" },
  size: { fontFamily: "var(--font-family-code)", fontSize: 12, textAlign: "end", color: "var(--precision-text3)" },
  small: { fontSize: 12, color: "var(--precision-text3)" },
  updated: {
    display: "block",
    overflow: "hidden",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  footnote: { margin: 0, paddingInline: 18, paddingBlock: 12, fontSize: 12, color: "var(--precision-text3)" },
  fieldset: { display: "flex", flexDirection: "column", gap: 4, margin: 0, borderWidth: 0, padding: 0 },
  member: { marginInlineStart: 24 },
  history: {
    display: "flex",
    width: 380,
    flexShrink: 0,
    flexDirection: "column",
    gap: 12,
    overflowY: "auto",
    borderInlineStartWidth: 1,
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    padding: 18,
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "ease-out",
  },
  versions: { display: "flex", maxHeight: 320, flexDirection: "column", gap: 8, overflowY: "auto" },
  version: {
    display: "flex",
    flexDirection: "column",
    alignItems: "flex-start",
    gap: 4,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    paddingInline: 12,
    paddingBlock: 10,
  },
  versionCurrent: { borderColor: "var(--precision-acc-line)" },
  versionPick: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    textAlign: "start",
    color: "var(--color-text-primary)",
  },
  versionSource: { display: "block", fontSize: 12, color: "var(--precision-text3)" },
  stack4: { display: "flex", flexDirection: "column", gap: 4 },
  diff: {
    margin: 0,
    maxHeight: 320,
    overflow: "auto",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    padding: 10,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    lineHeight: "17px",
    whiteSpace: "pre-wrap",
  },
  added: { backgroundColor: "var(--color-overlay-hover)", color: "var(--color-text-accent)" },
  removed: { backgroundColor: "var(--color-overlay-hover)", color: "var(--color-text-error)" },
  same: { color: "var(--color-text-secondary)" },
  editorShell: { display: "flex", minHeight: 0, flex: 1 },
  editorMain: { display: "flex", minWidth: 0, flex: 1, flexDirection: "column" },
  back: {
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    textDecoration: "none",
  },
  truncate: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  versionTag: {
    borderRadius: 4,
    backgroundColor: "var(--color-overlay-hover)",
    paddingInline: 6,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  blocked: { opacity: 0.6 },
  editor: {
    display: "flex",
    minHeight: 0,
    flex: 1,
    flexDirection: "column",
    gap: 14,
    overflowY: "auto",
    paddingInline: 22,
    paddingBlock: 20,
  },
  notice: { display: "flex", flexDirection: "column", gap: 8 },
  reason: { margin: 0, color: "var(--color-text-secondary)" },
  alert: { margin: 0, color: "var(--color-text-error)" },
  amber: { margin: 0, color: "var(--color-text-warning)" },
  start: { alignSelf: "flex-start" },
  panes: { display: "grid", gap: 12 },
  panesOne: { gridTemplateColumns: "minmax(0, 1fr)" },
  panesTwo: { gridTemplateColumns: "repeat(2, minmax(0, 1fr))" },
  source: { fontFamily: "var(--font-family-code)", fontSize: 12, lineHeight: 1.7 },
  preview: {
    overflowY: "auto",
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 18,
    paddingBlock: 16,
    lineHeight: 1.65,
  },
  meters: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: 20,
    rowGap: 4,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  meterLine: { display: "flex", alignItems: "center", gap: 8 },
  over: { color: "var(--color-text-error)" },
  close: { color: "var(--color-text-warning)" },
  bar: {
    display: "inline-block",
    width: 120,
    height: 3,
    overflow: "hidden",
    borderRadius: 2,
    backgroundColor: "var(--color-overlay-hover)",
    verticalAlign: "middle",
  },
  fill: { display: "block", height: "100%" },
  toneOver: { backgroundColor: "var(--color-text-error)" },
  toneClose: { backgroundColor: "var(--color-text-warning)" },
  toneCalm: { backgroundColor: "var(--color-text-accent)" },
  company: { marginInlineStart: 8, fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  appliesRow: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 12,
    paddingBlock: 10,
  },
  appliesText: { minWidth: 0, flex: 1, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
});

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
    <FormDialog
      title="Add starter set"
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button
            variant="primary"
            label={busy ? "Adding…" : `Add ${ticked.size}`}
            isDisabled={busy || ticked.size === 0}
            onClick={() => void add()}
          />
        </>
      }
    >
      <p {...stylex.props(styles.sub)}>
        Templates to start from. They become ordinary guidelines without links: link them to use them.
      </p>
      {templates === null && !error ? <Loading /> : null}
      {error ? <Problem>{error}</Problem> : null}
      {templates?.map((t) => (
        <CheckboxInput
          key={t.key}
          label={t.title}
          description={`${count(t.chars)} characters · for ${t.level === "project" ? "a project" : "employees"}${t.in_library ? " · already in the library" : ""}`}
          isDisabled={t.in_library}
          value={ticked.has(t.key)}
          onChange={(checked) => {
            const next = new Set(ticked);
            if (checked) next.add(t.key);
            else next.delete(t.key);
            setTicked(next);
          }}
        />
      ))}
    </FormDialog>
  );
}

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
  const workspace = useWorkspace().id;
  const inHere = useMemo(
    () => guidelinesIn(source.data ?? [], workspace, names.employees),
    [source.data, workspace, names.employees],
  );
  const list = useMemo(() => shown(inHere, query, sort, names), [inHere, query, sort, names]);
  const writable = canEdit(user);
  const open = (id: string) => void navigate({ to: "/knowledge/guidelines/$id", params: { id } });
  useAction("NewGuideline", () => open("new"), writable);
  useAction("FindGuideline", () => {
    setArchived(false);
    search.current?.focus();
  });

  if (!features.includes("guidelines")) {
    return (
      <Page>
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />} />
        <p {...stylex.props(styles.message)}>This server does not have guidelines yet. Update the server.</p>
      </Page>
    );
  }

  const pending = (g: GuidelineView) => (proposals.data ?? []).filter((p) => p.guideline_id === g.id);
  const showBanner = !archived && writable && bannerDue(active.data ?? [], user?.id ?? null, done);
  const head = (key: Sort, label: string) => (
    <button type="button" onClick={() => setSort(key)} {...stylex.props(styles.sort)}>
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
    <Page>
      <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
        <TextInput
          ref={search}
          label="Find a guideline"
          isLabelHidden
          size="sm"
          width={220}
          placeholder="Find… (@Forge, ▸project)"
          value={query}
          onChange={setQuery}
        />
        <ToggleButton size="sm" label="Archived" isPressed={archived} onPressedChange={setArchived} />
        {writable ? <Button size="sm" variant="primary" label="New guideline" onClick={() => open("new")} /> : null}
      </PageHead>
      {showBanner ? (
        <div {...stylex.props(styles.bannerSlot)}>
          <Banner
            status="info"
            title={BANNER}
            endContent={
              <Button
                size="sm"
                label="Got it"
                onClick={() => {
                  const next = [...done, user?.id ?? ""];
                  setDone(next);
                  platform.storage.set("guidelines_banner_done", JSON.stringify(next));
                }}
              />
            }
          />
        </div>
      ) : null}
      {!writable ? <p {...stylex.props(styles.readOnly)}>{READ_ONLY}</p> : null}
      <div {...stylex.props(styles.scroll)}>
        {source.isPending ? (
          <Loading />
        ) : (source.data ?? []).length === 0 ? (
          archived ? (
            <p {...stylex.props(styles.empty)}>Nothing is archived.</p>
          ) : (
            <Empty title="No guidelines yet" description={`${EMPTY_ABOUT} ${EMPTY_PROPOSE}`}>
              {writable ? (
                <div {...stylex.props(styles.buttons)}>
                  <Button variant="primary" label="+ New guideline" onClick={() => open("new")} />
                  <Button label="Add starter set…" onClick={() => setStarter(true)} />
                </div>
              ) : null}
            </Empty>
          )
        ) : (
          <>
            <Table density="compact" hasHover xstyle={styles.table}>
              <TableHeader>
                <TableRow isHeaderRow>
                  <TableHeaderCell xstyle={styles.headCell}>{head("title", "Title")}</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Applies to</TableHeaderCell>
                  <TableHeaderCell xstyle={[styles.headCell, styles.sizeColumn]}>
                    {head("size", "Size")}
                  </TableHeaderCell>
                  <TableHeaderCell xstyle={[styles.headCell, styles.versionColumn]}>Version</TableHeaderCell>
                  <TableHeaderCell xstyle={[styles.headCell, styles.updatedColumn]}>
                    {head("updated", "Updated")}
                  </TableHeaderCell>
                </TableRow>
              </TableHeader>
              <TableBody>
                {list.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5}>
                      <span {...stylex.props(styles.small)}>No guideline matches.</span>
                    </TableCell>
                  </TableRow>
                ) : null}
                {list.map((g) => {
                  const applies = appliesTo(g.links, names);
                  const waiting = Math.max(g.open_proposals ?? 0, pending(g).length);
                  return (
                    <TableRow key={g.id} {...stylex.props(styles.row)}>
                      <TableCell>
                        <Link to="/knowledge/guidelines/$id" params={{ id: g.id }} {...stylex.props(styles.link)}>
                          {g.title}
                          {workspace !== null && guidelineReach(g, workspace, names.employees) === "company" ? (
                            <span
                              title="For the whole company, not only this workspace"
                              {...stylex.props(styles.company)}
                            >
                              Company
                            </span>
                          ) : null}
                          {waiting > 0 ? (
                            <span
                              title={waiting === 1 ? "1 proposal waits for you" : `${waiting} proposals wait for you`}
                              {...stylex.props(styles.waiting)}
                            >
                              ◆{waiting}
                            </span>
                          ) : null}
                        </Link>
                      </TableCell>
                      <TableCell>
                        <span
                          title={applies.all}
                          {...stylex.props(styles.applies, g.links.length === 0 ? styles.dim : styles.appliesOn)}
                        >
                          {applies.shown}
                        </span>
                      </TableCell>
                      <TableCell>
                        <span {...stylex.props(styles.size)}>{count(g.chars)}</span>
                      </TableCell>
                      <TableCell>
                        <span {...stylex.props(styles.small)}>v{g.version}</span>
                      </TableCell>
                      <TableCell>
                        {archived && writable ? (
                          <Button size="sm" label="Restore" onClick={() => void restore(g)} />
                        ) : (
                          <span {...stylex.props(styles.updated)}>
                            {g.updated_by
                              ? g.updated_by.id === user?.id
                                ? "You"
                                : g.updated_by.username
                              : "Starter set"}{" "}
                            · {shortDay(g.updated_at_ms)}
                          </span>
                        )}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
            <p {...stylex.props(styles.footnote)}>
              {archived
                ? `${list.length} archived · no run gets these; Restore puts one back`
                : `${list.length} guideline${list.length === 1 ? "" : "s"} · ${ABOUT.charAt(0).toLowerCase()}${ABOUT.slice(1, -1)}. Size in characters (max ${count(TEXT_LIMIT)} each)`}
            </p>
          </>
        )}
      </div>
      {starter ? <StarterDialog onClose={() => setStarter(false)} /> : null}
    </Page>
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
    <FormDialog
      title={title}
      width={640}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button
            variant="primary"
            label={busy ? "Saving…" : saveLabel}
            isDisabled={busy}
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
          />
        </>
      }
    >
      <Eyebrow>EMPLOYEES</Eyebrow>
      {grouped.map((g) => {
        const state = tri(g.members, ticked);
        return (
          <fieldset key={g.label} {...stylex.props(styles.fieldset)}>
            <CheckboxInput
              label={g.label}
              description={`${state.ticked} of ${state.of}`}
              value={state.state === "all" ? true : state.state === "some" ? "indeterminate" : false}
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
            {g.members.map((id) => {
              const e = names.employees.find((x) => x.id === id);
              const blocked = blockedEmployee(id);
              return (
                <div key={id} {...stylex.props(styles.member)}>
                  <CheckboxInput
                    label={`${e?.name ?? ""}${e?.status === "paused" ? " (paused)" : e?.status === "archived" ? " (archived)" : ""}`}
                    isDisabled={blocked !== null}
                    {...(blocked ? { disabledMessage: blockSentence(blocked, names) } : {})}
                    value={ticked.has(`employee:${id}`)}
                    onChange={(checked) => toggle(`employee:${id}`, checked)}
                  />
                </div>
              );
            })}
          </fieldset>
        );
      })}
      <Hint>{GROUP_NOTE}</Hint>
      <Eyebrow>PROJECTS</Eyebrow>
      {names.projects.map((p) => {
        const block = projectBlock(sets, p.id, where, names.employees, guideline, size);
        const key = `project:${p.id}`;
        const blocked = !before.has(key) && !ticked.has(key) && block && block.chars > RUN_LIMIT ? block : null;
        return (
          <CheckboxInput
            key={p.id}
            label={p.name}
            description="everyone working in this project"
            isDisabled={blocked !== null}
            {...(blocked ? { disabledMessage: blockSentence(blocked, names) } : {})}
            value={ticked.has(key)}
            onChange={(checked) => toggle(key, checked)}
          />
        );
      })}
      {largest ? (
        <Hint>
          largest: {blockWhere(largest, names)} · {count(largest.chars)}
        </Hint>
      ) : null}
      {error ? <Problem>{error}</Problem> : null}
    </FormDialog>
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
    <aside aria-label="History" {...stylex.props(styles.history)}>
      <Eyebrow>HISTORY · {title.toUpperCase()}</Eyebrow>
      {versions.isPending ? <Loading /> : null}
      <div {...stylex.props(styles.versions)}>
        {versions.data?.map((v) => (
          <div key={v.version} {...stylex.props(styles.version, v.version === current && styles.versionCurrent)}>
            <button
              type="button"
              aria-pressed={pick === v.version}
              onClick={() => setPick(pick === v.version ? null : v.version)}
              {...stylex.props(styles.versionPick)}
            >
              Version {v.version}
              {v.version === current ? " · current" : ""} · {v.by?.username ?? "Aurelius"} · {shortDay(v.created_at_ms)}
              <span {...stylex.props(styles.versionSource)}>{sourceLabel(v, proposals.data ?? [], names)}</span>
            </button>
            {canRestore && v.version !== current ? (
              <Button size="sm" label="Restore" onClick={() => onRestore(v.version)} />
            ) : null}
          </div>
        ))}
      </div>
      {picked ? (
        <div {...stylex.props(styles.stack4)}>
          <Eyebrow>Version {picked.version} → what the editor holds</Eyebrow>
          <Diff lines={diff} />
        </div>
      ) : null}
      <Hint>Links have no versions: only the title and the text do.</Hint>
    </aside>
  );
}

function Diff({ lines }: { lines: ReturnType<typeof lineDiff> }) {
  return (
    <pre {...stylex.props(styles.diff)}>
      {lines.map((l, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: a diff is drawn whole.
          key={i}
          {...stylex.props(l.mark === "+" ? styles.added : l.mark === "-" ? styles.removed : styles.same)}
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
    <FormDialog
      title="Reject this proposal?"
      width={480}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="destructive" label="Reject" onClick={() => void reject()} />
        </>
      }
    >
      <TextArea
        hasAutoFocus
        label={`Note for ${who}`}
        rows={3}
        placeholder="Optional: why, for the employee who proposed it"
        value={note}
        onChange={setNote}
      />
      <Hint>{who} gets your decision and the note.</Hint>
    </FormDialog>
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
  const tone = (l: string) => (l === "over" ? styles.toneOver : l === "close" ? styles.toneClose : styles.toneCalm);
  const bar = (value: number, max: number, l: string) => (
    <span {...stylex.props(styles.bar)}>
      <span {...stylex.props(styles.fill, tone(l))} style={{ width: `${Math.min(100, (value / max) * 100)}%` }} />
    </span>
  );
  const whoProposed = proposal
    ? (names.employees.find((e) => e.id === proposal.proposed_by)?.name ?? "An employee")
    : "";

  if (!isNew && detail.error) {
    return (
      <Page>
        <PageHead
          title={
            <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} {...stylex.props(styles.back)}>
              ‹ Guidelines
            </Link>
          }
        />
        <Problem>{detail.error.message}</Problem>
      </Page>
    );
  }

  return (
    <div {...stylex.props(styles.editorShell)}>
      <div {...stylex.props(styles.editorMain)}>
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
                {...stylex.props(styles.back)}
              >
                ‹ Guidelines
              </Link>
              <span {...stylex.props(styles.truncate)}>· {heading}</span>
              {current ? <span {...stylex.props(styles.versionTag)}>v{current.version}</span> : null}
            </>
          }
        >
          {!isNew && !proposal ? (
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={history}
              label={`History ${history ? "◂" : "▸"}`}
              onClick={() => setHistory(!history)}
            />
          ) : null}
          {!isNew && !proposal && writable ? (
            <Button size="sm" variant="ghost" label="Archive…" onClick={() => setConfirm("archive")} />
          ) : null}
          <Button size="sm" label="Cancel" onClick={tryLeave} />
          {writable ? (
            <Button
              size="sm"
              variant="primary"
              label={busy ? "Saving…" : proposal ? "Approve" : "Save"}
              onClick={() => void save()}
              xstyle={problem || busy ? styles.blocked : undefined}
            />
          ) : null}
        </PageHead>
        <div {...stylex.props(styles.editor)}>
          {archived ? (
            <p {...stylex.props(styles.reason)}>
              Archived: no run gets this. Restore it from the library to change it.
            </p>
          ) : null}
          {proposal ? (
            <Banner status="warning" title={proposalHeadline(proposal, current, names)} collapsible={false}>
              <div {...stylex.props(styles.notice)}>
                <Eyebrow>GUIDELINE PROPOSED · WAITING FOR YOU</Eyebrow>
                {proposal.reason ? <p {...stylex.props(styles.reason)}>{proposal.reason}</p> : null}
                {isStale ? (
                  <p role="alert" {...stylex.props(styles.alert)}>
                    “{current?.title}” changed since this was proposed (version {isStale.base} → {isStale.now}), so it
                    can't be approved. Reject it; {whoProposed} can propose again on the new text.
                  </p>
                ) : approveOff ? (
                  <p {...stylex.props(styles.amber)}>{approveOff}</p>
                ) : null}
                <div>
                  <Button size="sm" variant="destructive" label="Reject" onClick={() => setRejecting(true)} />
                </div>
              </div>
            </Banner>
          ) : null}
          {conflict !== null ? (
            <Banner
              status="warning"
              role="alert"
              title={`Version ${conflict} was saved while you were editing. Nothing of yours was saved.`}
              collapsible={false}
            >
              <div {...stylex.props(styles.notice)}>
                <div {...stylex.props(styles.buttons)}>
                  <Button
                    size="sm"
                    aria-expanded={compare}
                    label={compare ? "Hide compare" : "Compare"}
                    onClick={() => setCompare(!compare)}
                  />
                  <Button size="sm" label="Discard mine" onClick={discardMine} />
                </div>
                {compare && detail.data ? (
                  <>
                    <Eyebrow>Version {detail.data.version.version} → yours</Eyebrow>
                    <Diff lines={lineDiff(detail.data.version.text, text)} />
                  </>
                ) : null}
              </div>
            </Banner>
          ) : null}
          <TextInput
            hasAutoFocus
            label="Title *"
            placeholder="Rust"
            isReadOnly={!writable}
            value={title}
            onChange={edit(setTitle)}
          />
          <ToggleButtonGroup
            label="View"
            size="sm"
            value={mode}
            onChange={(v) => v && setMode(v as typeof mode)}
            xstyle={styles.start}
          >
            <ToggleButton value="write" label="Write" />
            <ToggleButton value="preview" label="Preview" />
            <ToggleButton value="side" label="Side by side" />
          </ToggleButtonGroup>
          <div
            {...stylex.props(styles.panes, mode === "side" ? styles.panesTwo : styles.panesOne)}
            style={{ minHeight: height }}
          >
            {mode !== "preview" ? (
              <TextArea
                label="Text"
                isLabelHidden
                isReadOnly={!writable}
                rows={Math.max(3, Math.floor((height - 28) / 20))}
                placeholder="The rules, in Markdown: ## headings and - lists work."
                controlXstyle={styles.source}
                value={text}
                onChange={edit(setText)}
              />
            ) : null}
            {mode !== "write" ? (
              <div {...stylex.props(styles.preview)} style={{ height }}>
                <Markdown text={safe} safe />
              </div>
            ) : null}
          </div>
          <div {...stylex.props(styles.meters)}>
            <span {...stylex.props(styles.meterLine)}>
              {bar(n, TEXT_LIMIT, counter)} {counterLine(n)} · {tokens(n)}
              {counterNote(n) ? (
                <span {...stylex.props(counter === "over" ? styles.over : styles.close)}>{counterNote(n)}</span>
              ) : null}
            </span>
            <span {...stylex.props(styles.meterLine)}>
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
          {message ? <Problem>{message}</Problem> : null}
          <div {...stylex.props(styles.appliesRow)}>
            <span {...stylex.props(styles.small)}>Applies to</span>
            <span title={appliesTo(linkList, names).all} {...stylex.props(styles.appliesText)}>
              {appliesTo(linkList, names).shown}
            </span>
            {writable ? <Button size="sm" label="Links…" onClick={() => setEditLinks(true)} /> : null}
          </div>
          <Hint>
            {affected.data
              ? `${affectedLine(affected.data)} Runs that already started keep version ${current?.version}.`
              : "Applies from the next run."}
          </Hint>
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
