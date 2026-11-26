import { Button } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { TextArea } from "@astryxdesign/core/TextArea";
import type { MemoryContextItem, MemoryView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { useConnection } from "../connection";
import { useEmployees, useMemory, useMemoryContext, useProjects } from "../data";
import { useAction } from "../keys/provider";
import { toast } from "../ui/toast";
import { consolidationScopes, memoryHistory, memoryKind, thousands } from "./knowledge-logic";
import { Confirm, Eyebrow, FormDialog, failureOf, Hint, hhmm, Loading, Problem, shortDay } from "./org-kit";
import { Grow, Meter, Segments, SettingsPage, Toolbar } from "./settings-frame";

type MemoryTab = "context" | "history";

const _PANEL = "memory-panel";
// Selector takes no empty value, so "no employee" needs a word of its own.
const _NO_EMPLOYEE = "no-employee";

const styles = stylex.create({
  entry: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
    paddingInline: 18,
    paddingBlock: 16,
  },
  selected: { backgroundColor: "var(--color-overlay-hover)" },
  meta: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  kind: {
    display: "inline-flex",
    height: 20,
    alignItems: "center",
    gap: 6,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 7,
    fontSize: 11,
    color: "var(--color-text-secondary)",
  },
  dot: { width: 6, height: 6, borderRadius: "50%", backgroundColor: "var(--precision-violet)" },
  badge: {
    display: "inline-flex",
    height: 20,
    alignItems: "center",
    borderRadius: 4,
    backgroundColor: "var(--color-overlay-hover)",
    paddingInline: 6,
    fontSize: 11,
    letterSpacing: "0.04em",
    color: "var(--precision-text3)",
  },
  badgeProposed: { color: "var(--color-text-accent)" },
  source: { marginInlineStart: "auto" },
  content: { margin: 0, fontSize: 14, lineHeight: 1.6, whiteSpace: "pre-wrap" },
  erased: { color: "var(--precision-text3)" },
  actions: { display: "flex", flexWrap: "wrap", gap: 8 },
  contextSection: { display: "flex", flexDirection: "column", gap: 4, paddingInline: 18, paddingBlock: 12 },
  contextRow: {
    display: "flex",
    alignItems: "flex-start",
    gap: 12,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
    paddingBlock: 8,
  },
  dim75: { opacity: 0.75 },
  dim55: { opacity: 0.55 },
  contextText: { minWidth: 0, flex: 1, whiteSpace: "pre-wrap" },
  contextSide: { flexShrink: 0, fontSize: 12, color: "var(--precision-text3)" },
  empty: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  split: { display: "flex", minHeight: 0, flex: 1 },
  main: { display: "flex", minWidth: 0, flexGrow: 999, flexShrink: 1, flexBasis: 520, flexDirection: "column" },
  panel: { minHeight: 0, flex: 1, overflowY: "auto" },
  column: { display: "flex", flexDirection: "column" },
  contextHead: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 12,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
    paddingInline: 18,
    paddingBlock: 12,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  meter: {
    display: "inline-block",
    width: 320,
    maxWidth: "100%",
    height: 4,
    overflow: "hidden",
    borderRadius: 2,
    backgroundColor: "var(--color-overlay-hover)",
  },
  meterFill: { display: "block", height: "100%", backgroundColor: "var(--color-accent)" },
  meterOver: { backgroundColor: "var(--color-text-error)" },
  active: { paddingInline: 18, paddingBlockStart: 12 },
  expander: { paddingInline: 10, paddingBlock: 8 },
  aside: {
    display: "flex",
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: 320,
    flexDirection: "column",
    gap: 14,
    overflowY: "auto",
    borderInlineStartWidth: 1,
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    padding: 18,
  },
  remember: {
    margin: 0,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingBlockStart: 14,
    fontSize: 12,
    lineHeight: 1.6,
    color: "var(--precision-text3)",
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
  budget: { display: "flex", flexDirection: "column", gap: 8, paddingBlock: 12, paddingInline: 16 },
  budgetLine: { display: "flex", flexWrap: "wrap", alignItems: "baseline", gap: 10 },
  mono: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  grow: { flex: 1 },
  sec: { display: "flex", flexDirection: "column", gap: 8 },
  secHead: { display: "flex", minHeight: 28, alignItems: "center", gap: 8, paddingInline: 2 },
  secTitle: { margin: 0, fontSize: 12.5, fontWeight: 600, color: "var(--color-text-secondary)" },
  secNote: { fontSize: 12, color: "var(--precision-text3)" },
  mem: {
    display: "flex",
    alignItems: "flex-start",
    gap: 12,
    paddingBlock: 11,
    paddingInline: 16,
    borderBlockStartWidth: { default: 1, ":first-child": 0 },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
  },
  memText: { display: "flex", minWidth: 0, flex: 1, flexDirection: "column", gap: 3 },
  memMeta: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 6,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  memKind: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 5,
    paddingInline: 7,
    paddingBlock: 1,
    backgroundColor: "var(--precision-sunk)",
    fontSize: 11,
    fontWeight: 600,
    color: "var(--color-text-secondary)",
  },
  memActions: { display: "flex", flexShrink: 0, gap: 4 },
  danger: { color: "var(--color-text-error)" },
  foot: {
    display: "flex",
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingBlock: 8,
    paddingInline: 10,
  },
});

export function Entry({
  m,
  selected,
  onSelect,
  children,
}: {
  m: MemoryView;
  selected: boolean;
  onSelect: () => void;
  children?: React.ReactNode;
}) {
  const badge =
    m.status === "proposed"
      ? "PROPOSAL"
      : m.status === "superseded"
        ? "SUPERSEDED"
        : m.status === "forgotten"
          ? "FORGOTTEN"
          : m.status === "rejected"
            ? "REJECTED"
            : null;
  const source = m.sources[0];
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: J and K do the same.
    <article
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
      {...stylex.props(styles.entry, selected && styles.selected)}
    >
      <div {...stylex.props(styles.meta)}>
        <span {...stylex.props(styles.kind)}>
          <span {...stylex.props(styles.dot)} />
          {memoryKind(m)}
        </span>
        {badge ? (
          <span {...stylex.props(styles.badge, m.status === "proposed" && styles.badgeProposed)}>{badge}</span>
        ) : null}
        {m.scope === "global" ? <span>· global</span> : m.employee_name ? <span>· {m.employee_name}</span> : null}
        {m.pinned ? <span>· pinned</span> : null}
        <span {...stylex.props(styles.source)}>
          {source ? `source: ${source.label} · ${hhmm(source.at_ms)}` : "no source"}
        </span>
      </div>
      <p {...stylex.props(styles.content)}>
        {m.content || <span {...stylex.props(styles.erased)}>(content erased)</span>}
      </p>
      {children ? <div {...stylex.props(styles.actions)}>{children}</div> : null}
    </article>
  );
}

export function ReviseDialog({ m, onClose }: { m: MemoryView; onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [text, setText] = useState(m.content);
  const save = async () => {
    try {
      await connection.write("memory.revise", { memory_id: m.id, content: text.trim() }, { retry: false });
      await client.invalidateQueries({ queryKey: ["memory"] });
      onClose();
    } catch (e) {
      toast(failureOf(e));
    }
  };
  return (
    <FormDialog
      title="Revise memory"
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button
            variant="primary"
            label="Save new version"
            isDisabled={text.trim() === ""}
            onClick={() => void save()}
          />
        </>
      }
    >
      <Hint>The old version stays visible in the history but is no longer used.</Hint>
      <TextArea hasAutoFocus isLabelHidden label="Memory" rows={5} value={text} onChange={setText} />
    </FormDialog>
  );
}

function _ContextRows({
  title,
  info,
  items,
  dim,
}: {
  title: string;
  info?: string;
  items: MemoryContextItem[];
  dim?: stylex.StyleXStyles;
}) {
  if (items.length === 0) return null;
  return (
    <section {...stylex.props(styles.contextSection)}>
      <Eyebrow {...(info ? { title: info } : {})}>{title}</Eyebrow>
      {info ? <Hint>{info}</Hint> : null}
      {items.map((i) => (
        <div key={i.memory_id} {...stylex.props(styles.contextRow, dim)}>
          <span {...stylex.props(styles.contextText)}>{i.content}</span>
          <span {...stylex.props(styles.contextSide)}>
            {i.label} · ~{i.tokens} tok
          </span>
        </div>
      ))}
    </section>
  );
}

/** One memory as a row: its text, kind and who it is for, and Pin, Revise and Forget. */
function MemoryRow({
  text,
  kind,
  who,
  m,
  onPin,
  onRevise,
  onForget,
}: {
  text: string;
  kind: string;
  who: string;
  m: MemoryView | undefined;
  onPin: (m: MemoryView) => void;
  onRevise: (m: MemoryView) => void;
  onForget: (m: MemoryView) => void;
}) {
  return (
    <div {...stylex.props(styles.mem)}>
      <div {...stylex.props(styles.memText)}>
        <span>{text || <span {...stylex.props(styles.erased)}>(content erased)</span>}</span>
        <span {...stylex.props(styles.memMeta)}>
          <span {...stylex.props(styles.memKind)}>{kind}</span>
          {who}
        </span>
      </div>
      {m && m.status === "active" ? (
        <div {...stylex.props(styles.memActions)}>
          <Button size="sm" variant="ghost" label={m.pinned ? "Unpin" : "Pin"} onClick={() => onPin(m)} />
          <Button size="sm" variant="ghost" label="Revise" onClick={() => onRevise(m)} />
          <Button size="sm" variant="ghost" label="Forget…" onClick={() => onForget(m)} xstyle={styles.danger} />
        </div>
      ) : null}
    </div>
  );
}

function MemorySection({
  title,
  note,
  action,
  children,
}: {
  title: string;
  note: string;
  action?: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <section {...stylex.props(styles.sec)}>
      <div {...stylex.props(styles.secHead)}>
        <h2 {...stylex.props(styles.secTitle)}>{title}</h2>
        <span {...stylex.props(styles.secNote)}>{note}</span>
        <span {...stylex.props(styles.grow)} />
        {action}
      </div>
      {children ? <div {...stylex.props(styles.card)}>{children}</div> : null}
    </section>
  );
}

const EVERYONE = "everyone";
const SHOWN_AT_FIRST = 5;

/** Remember: a kind, for whom, and the text. */
function RememberDialog({
  project,
  projectName,
  staff,
  onClose,
}: {
  project: string;
  projectName: string;
  staff: { id: string; name: string }[];
  onClose: () => void;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const [text, setText] = useState("");
  const [kind, setKind] = useState<"project_knowledge" | "decision">("project_knowledge");
  const [forWhom, setForWhom] = useState(EVERYONE);
  const save = async () => {
    try {
      await connection.write(
        "memory.remember",
        {
          scope: forWhom === EVERYONE ? { kind: "project", id: project } : { kind: "employee", id: forWhom },
          kind,
          content: text.trim(),
          pinned: false,
        },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["memory"] });
      toast("Remembered, with you as the source.");
      onClose();
    } catch (e) {
      toast(failureOf(e));
    }
  };
  return (
    <FormDialog
      title={`Remember for ${projectName}`}
      width={420}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Remember" isDisabled={text.trim() === ""} onClick={() => void save()} />
        </>
      }
    >
      <SegmentedControl label="Kind" size="sm" value={kind} onChange={(v) => setKind(v as typeof kind)}>
        <SegmentedControlItem value="project_knowledge" label="Knowledge" />
        <SegmentedControlItem value="decision" label="Decision" />
      </SegmentedControl>
      <Selector
        label="For"
        size="sm"
        value={forWhom}
        onChange={setForWhom}
        options={[{ value: EVERYONE, label: "Everyone" }, ...staff.map((e) => ({ value: e.id, label: e.name }))]}
      />
      <TextArea
        hasAutoFocus
        label="Text"
        rows={4}
        placeholder="What every run in this workspace should know"
        value={text}
        onChange={setText}
      />
    </FormDialog>
  );
}

/** Settings › Company › Memory: what a run in a workspace gets, and every memory with its actions. */
export function MemoryBody({ projectId, start = null }: { projectId: string | null; start?: string | null }) {
  const connection = useConnection();
  const client = useQueryClient();
  const projects = useProjects();
  const employees = useEmployees();
  const [picked, setPicked] = useState<string | null>(start);
  const open = (projects.data ?? []).filter((p) => p.status !== "archived");
  const project = picked ?? projectId ?? open[0]?.id ?? null;
  const projectName = open.find((p) => p.id === project)?.name ?? "this workspace";
  const memory = useMemory(project);
  const staff = useMemo(
    () => (employees.data ?? []).filter((e) => e.projects_active.includes(project ?? "")),
    [employees.data, project],
  );
  const [who, setWho] = useState<string>(EVERYONE);
  const contextEmployee = who === EVERYONE ? null : who;
  const context = useMemoryContext(project, contextEmployee);
  const [tab, setTab] = useState<MemoryTab>("context");
  const [remembering, setRemembering] = useState(false);
  const [revising, setRevising] = useState<MemoryView | null>(null);
  const [forgetting, setForgetting] = useState<MemoryView | null>(null);
  const [moreRelevant, setMoreRelevant] = useState(false);
  const [overOpen, setOverOpen] = useState(false);
  const items = memory.data ?? [];
  const history = useMemo(() => memoryHistory(items), [items]);
  const byId = useMemo(() => new Map(items.map((m) => [m.id, m])), [items]);
  const refresh = () => client.invalidateQueries({ queryKey: ["memory"] });
  const act = async (run: () => Promise<unknown>, done?: string) => {
    try {
      await run();
      await refresh();
      if (done) toast(done);
    } catch (e) {
      toast(failureOf(e));
    }
  };
  const consolidate = async () => {
    if (!project) return;
    try {
      let queued = false;
      for (const scope of consolidationScopes(project, items)) {
        const receipt = await connection.write("memory.consolidate", { scope }, { retry: false });
        const made = receipt.result;
        if (made.kind === "command" && made.value.type === "memory_consolidation_queued" && made.value.queued)
          queued = true;
      }
      toast(
        queued
          ? "Consolidating memory in the background. Merged memories move to History."
          : "A consolidation of this memory is already running.",
      );
    } catch (e) {
      toast(failureOf(e));
    }
  };
  const pin = (m: MemoryView) =>
    act(
      () => connection.write("memory.pin", { memory_id: m.id, pinned: !m.pinned }, { retry: false }),
      m.pinned ? "Unpinned." : "Pinned: always included as context.",
    );
  useAction("Filter1", () => setTab("context"));
  useAction("Filter3", () => setTab("history"));
  const ctx = context.data;
  const place = (p: string) => (ctx?.items ?? []).filter((i) => i.placement === p);
  const inContext = new Set((ctx?.items ?? []).map((i) => i.memory_id));
  const others = items.filter((m) => m.status === "active" && !inContext.has(m.id));
  const whoOf = (name: string | null | undefined, pinned: boolean, at?: number) =>
    [name ?? "Everyone", pinned ? "pinned" : null, at ? shortDay(at) : null].filter(Boolean).join(" · ");
  const row = (i: MemoryContextItem) => {
    const m = byId.get(i.memory_id);
    return (
      <MemoryRow
        key={i.memory_id}
        text={i.content}
        kind={memoryKind(m ?? { kind: i.kind })}
        who={whoOf(i.employee_name, i.pinned, m?.created_at_ms)}
        m={m}
        onPin={(x) => void pin(x)}
        onRevise={setRevising}
        onForget={setForgetting}
      />
    );
  };
  const relevant = place("when_relevant");
  const over = place("over_budget");
  return (
    <SettingsPage title="Memory" width="mid">
      {!project && projects.data ? (
        <p {...stylex.props(styles.empty)}>Make a workspace first: memory belongs to a workspace.</p>
      ) : (
        <>
          <Toolbar>
            <Selector
              label="Workspace"
              isLabelHidden
              size="sm"
              variant="secondary"
              value={project ?? ""}
              onChange={(v) => {
                setPicked(v);
                setWho(EVERYONE);
              }}
              options={open.map((p) => ({ value: p.id, label: p.name }))}
            />
            <Selector
              label="Context of"
              isLabelHidden
              size="sm"
              variant="secondary"
              value={who}
              onChange={setWho}
              options={[{ value: EVERYONE, label: "Everyone" }, ...staff.map((e) => ({ value: e.id, label: e.name }))]}
            />
            <Segments
              label="Show"
              options={[
                { value: "context", label: "In use" },
                { value: "history", label: `History ${history.length}` },
              ]}
              value={tab}
              onChange={setTab}
            />
            <Grow />
            <Button
              size="sm"
              label="Consolidate"
              tooltip="Merges memories that say the same; the old ones go to History"
              isDisabled={!project}
              onClick={() => void consolidate()}
            />
            <Button
              size="sm"
              variant="primary"
              label="Remember"
              isDisabled={!project}
              onClick={() => setRemembering(true)}
            />
          </Toolbar>
          {memory.isPending ? <Loading /> : null}
          {tab === "history" && memory.data ? (
            history.length === 0 ? (
              <p {...stylex.props(styles.empty)}>Nothing superseded, rejected or forgotten yet.</p>
            ) : (
              <div {...stylex.props(styles.card)}>
                {history.map((m) => (
                  <MemoryRow
                    key={m.id}
                    text={m.content}
                    kind={`${memoryKind(m)} · ${m.status}`}
                    who={whoOf(m.employee_name, m.pinned, m.created_at_ms)}
                    m={undefined}
                    onPin={() => undefined}
                    onRevise={() => undefined}
                    onForget={() => undefined}
                  />
                ))}
              </div>
            )
          ) : null}
          {tab === "context" ? (
            <>
              {ctx ? (
                <div {...stylex.props(styles.card, styles.budget)}>
                  <div {...stylex.props(styles.budgetLine)}>
                    <b>What a run in {projectName} gets</b>
                    <span {...stylex.props(styles.grow)} />
                    <span {...stylex.props(styles.mono)}>
                      {thousands(ctx.always_tokens)} / {thousands(ctx.memory_budget_tokens)} tokens
                    </span>
                  </div>
                  <Meter used={ctx.always_tokens} limit={ctx.memory_budget_tokens} label="Memory in context" />
                  <span {...stylex.props(styles.secNote)}>
                    {ctx.system_tokens
                      ? `Plus the instructions (≈ ${thousands(ctx.system_tokens)} tokens) and `
                      : "Plus "}
                    the guidelines of the employee.
                  </span>
                </div>
              ) : null}
              {context.error ? <Problem>{context.error.message}</Problem> : null}
              {ctx && ctx.items.length === 0 && others.length === 0 ? (
                <p {...stylex.props(styles.empty)}>No memory yet. Use Remember.</p>
              ) : null}
              {place("always").length > 0 ? (
                <MemorySection title="Always in context" note={String(place("always").length)}>
                  {place("always").map(row)}
                </MemorySection>
              ) : null}
              {relevant.length > 0 ? (
                <MemorySection title="Only when it fits the question" note={String(relevant.length)}>
                  {(moreRelevant ? relevant : relevant.slice(0, SHOWN_AT_FIRST)).map(row)}
                  {relevant.length > SHOWN_AT_FIRST && !moreRelevant ? (
                    <div {...stylex.props(styles.foot)}>
                      <Button
                        size="sm"
                        variant="ghost"
                        label={`Show ${relevant.length - SHOWN_AT_FIRST} more`}
                        onClick={() => setMoreRelevant(true)}
                      />
                    </div>
                  ) : null}
                </MemorySection>
              ) : null}
              {over.length > 0 ? (
                <MemorySection
                  title="Does not fit"
                  note={`${over.length} · left out until there is room`}
                  action={
                    <Button
                      size="sm"
                      variant="ghost"
                      label={overOpen ? "Hide" : "Show"}
                      onClick={() => setOverOpen(!overOpen)}
                    />
                  }
                >
                  {overOpen ? over.map(row) : null}
                </MemorySection>
              ) : null}
              {others.length > 0 ? (
                <MemorySection title="For other employees" note={String(others.length)}>
                  {others.map((m) => (
                    <MemoryRow
                      key={m.id}
                      text={m.content}
                      kind={memoryKind(m)}
                      who={whoOf(m.employee_name, m.pinned, m.created_at_ms)}
                      m={m}
                      onPin={(x) => void pin(x)}
                      onRevise={setRevising}
                      onForget={setForgetting}
                    />
                  ))}
                </MemorySection>
              ) : null}
            </>
          ) : null}
        </>
      )}
      {remembering && project ? (
        <RememberDialog
          project={project}
          projectName={projectName}
          staff={staff}
          onClose={() => setRemembering(false)}
        />
      ) : null}
      {revising ? <ReviseDialog m={revising} onClose={() => setRevising(null)} /> : null}
      {forgetting ? (
        <Confirm
          danger
          title="Forget this memory?"
          body="It is never used as context again, leaves the search index and its content is erased. The timeline keeps the fact that it existed."
          action="Forget"
          onClose={() => setForgetting(null)}
          onConfirm={() => {
            const m = forgetting;
            setForgetting(null);
            void act(
              () => connection.write("memory.forget", { memory_id: m.id }, { retry: false }),
              "Forgotten: no longer in context or the search index; content erased.",
            );
          }}
        />
      ) : null}
    </SettingsPage>
  );
}
