import { Button } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { TextArea } from "@astryxdesign/core/TextArea";
import type { MemoryContextItem, MemoryView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { useConnection } from "../connection";
import { useEmployees, useMemory, useMemoryContext, useProjects } from "../data";
import { useAction } from "../keys/provider";
import { toast } from "../ui/toast";
import { consolidationScopes, memoryHistory, memoryKind, memoryProposals, thousands } from "./knowledge-logic";
import {
  Confirm,
  Eyebrow,
  FormDialog,
  failureOf,
  Hint,
  hhmm,
  KnowledgeTabs,
  Loading,
  Page,
  PageHead,
  Problem,
} from "./org-kit";

type MemoryTab = "context" | "proposals" | "history";

const PANEL = "memory-panel";
// Selector takes no empty value, so "no employee" needs a word of its own.
const NO_EMPLOYEE = "no-employee";

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
});

const NOTE =
  "Employees propose memories during runs. Proposals wait here until you accept or reject them; accepted ones go into every run in this project.";

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

function ContextRows({
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

export function MemoryBody({
  projectId,
  chrome = true,
  start = null,
}: {
  projectId: string | null;
  chrome?: boolean;
  start?: string | null;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const projects = useProjects();
  const employees = useEmployees();
  const [picked, setPicked] = useState<string | null>(start);
  const project = projectId ?? picked ?? projects.data?.find((p) => p.status !== "archived")?.id ?? null;
  const memory = useMemory(project);
  const staff = useMemo(
    () => (employees.data ?? []).filter((e) => e.projects_active.includes(project ?? "")),
    [employees.data, project],
  );
  const [who, setWho] = useState<string | null | undefined>(undefined);
  const contextEmployee = who === undefined ? (staff[0]?.id ?? null) : who;
  const context = useMemoryContext(project, contextEmployee);
  const [tab, setTab] = useState<MemoryTab>("proposals");
  const [text, setText] = useState("");
  const [kind, setKind] = useState<"project_knowledge" | "decision">("project_knowledge");
  const [revising, setRevising] = useState<MemoryView | null>(null);
  const [forgetting, setForgetting] = useState<MemoryView | null>(null);
  const [overOpen, setOverOpen] = useState(false);
  const items = memory.data ?? [];
  const proposals = useMemo(() => memoryProposals(items), [items]);
  const history = useMemo(() => memoryHistory(items), [items]);
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
  const decide = (m: MemoryView, accept: boolean) =>
    act(
      () => connection.write("memory.decide", { memory_id: m.id, accept }, { retry: false }),
      accept ? "Proposal accepted; it is used as context from now on." : "Proposal rejected.",
    );
  const remember = () => {
    if (!project || text.trim() === "") return;
    const content = text.trim();
    setText("");
    void act(
      () =>
        connection.write(
          "memory.remember",
          { scope: { kind: "project", id: project }, kind, content, pinned: false },
          { retry: false },
        ),
      "Saved to project memory, with you as the source.",
    );
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
  const shownItems =
    tab === "proposals" ? proposals : tab === "history" ? history : items.filter((m) => m.status === "active");
  const [chosen, setChosen] = useState<string | null>(null);
  const current = shownItems.find((m) => m.id === chosen) ?? shownItems[0] ?? null;
  const step = (by: number) => {
    const next = shownItems[(current ? shownItems.indexOf(current) : -1) + by];
    if (next) setChosen(next.id);
  };
  useAction("SelectNext", () => step(1));
  useAction("SelectPrev", () => step(-1));
  useAction("AcceptMemory", () => {
    if (current?.status === "proposed") void decide(current, true);
    else toast("Select a proposal to accept (filter 2 shows only proposals).");
  });
  useAction("RemoveMemory", () => {
    if (current?.status === "proposed") void decide(current, false);
    else if (current?.status === "active") setForgetting(current);
    else if (current) toast("This item is no longer active.");
  });
  const pin = (m: MemoryView) =>
    act(
      () => connection.write("memory.pin", { memory_id: m.id, pinned: !m.pinned }, { retry: false }),
      m.pinned ? "Unpinned." : "Pinned: always included as context.",
    );
  useAction("TogglePin", () => {
    if (current) void pin(current);
  });
  useAction("Filter1", () => setTab("context"));
  useAction("Filter2", () => setTab("proposals"));
  useAction("Filter3", () => setTab("history"));
  const ctx = context.data;
  const place = (p: string) => (ctx?.items ?? []).filter((i) => i.placement === p);
  const budget = Math.max(1, ctx?.memory_budget_tokens ?? 1);
  const over = (ctx?.always_tokens ?? 0) > budget;
  const tabs: { key: MemoryTab; label: string; n?: number }[] = [
    { key: "context", label: "In context" },
    { key: "proposals", label: "Proposals", n: proposals.length },
    { key: "history", label: "History", n: history.length },
  ];
  return (
    <Page>
      {chrome ? (
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
          <Selector
            label="Memory of"
            isLabelHidden
            size="sm"
            value={project ?? ""}
            onChange={(v) => {
              setPicked(v);
              setWho(undefined);
            }}
            options={(projects.data ?? [])
              .filter((p) => p.status !== "archived")
              .map((p) => ({ value: p.id, label: p.name }))}
          />
          <Button size="sm" label="Consolidate" isDisabled={!project} onClick={() => void consolidate()} />
        </PageHead>
      ) : null}
      {!project && projects.data ? (
        <p {...stylex.props(styles.empty)}>Make a project first: memory belongs to a project.</p>
      ) : (
        <div {...stylex.props(styles.split)}>
          <div {...stylex.props(styles.main)}>
            <TabList role="tablist" aria-label="Memory" hasDivider value={tab} onChange={(v) => setTab(v as MemoryTab)}>
              {tabs.map((t) => (
                <Tab
                  key={t.key}
                  value={t.key}
                  label={t.label}
                  panelId={PANEL}
                  {...(t.n ? { endContent: <span>{t.n}</span> } : {})}
                />
              ))}
            </TabList>
            <div id={PANEL} role="tabpanel" {...stylex.props(styles.panel)}>
              {memory.isPending ? <Loading /> : null}
              {tab === "proposals" && memory.data ? (
                proposals.length === 0 ? (
                  <p {...stylex.props(styles.empty)}>No proposals.</p>
                ) : (
                  proposals.map((m) => (
                    <Entry key={m.id} m={m} selected={m.id === current?.id} onSelect={() => setChosen(m.id)}>
                      <Button size="sm" variant="primary" label="Accept" onClick={() => void decide(m, true)} />
                      <Button size="sm" label="Reject" onClick={() => void decide(m, false)} />
                    </Entry>
                  ))
                )
              ) : null}
              {tab === "history" && memory.data ? (
                history.length === 0 ? (
                  <p {...stylex.props(styles.empty)}>Nothing superseded, rejected or forgotten yet.</p>
                ) : (
                  history.map((m) => (
                    <Entry key={m.id} m={m} selected={m.id === current?.id} onSelect={() => setChosen(m.id)} />
                  ))
                )
              ) : null}
              {tab === "context" ? (
                <div {...stylex.props(styles.column)}>
                  <div {...stylex.props(styles.contextHead)}>
                    <Selector
                      label="Context of"
                      size="sm"
                      value={contextEmployee ?? NO_EMPLOYEE}
                      onChange={(v) => setWho(v === NO_EMPLOYEE ? null : v)}
                      options={[
                        ...staff.map((e) => ({ value: e.id, label: e.name })),
                        { value: NO_EMPLOYEE, label: "No employee" },
                      ]}
                    />
                    {ctx ? (
                      <>
                        <span>
                          {ctx.system_tokens ? `Instructions ~${thousands(ctx.system_tokens)} tokens · ` : ""}Memory{" "}
                          {thousands(ctx.always_tokens)} / {thousands(ctx.memory_budget_tokens)} tokens
                        </span>
                        <span {...stylex.props(styles.meter)}>
                          <span
                            {...stylex.props(styles.meterFill, over && styles.meterOver)}
                            style={{ width: `${Math.min(100, (ctx.always_tokens / budget) * 100)}%` }}
                          />
                        </span>
                      </>
                    ) : null}
                  </div>
                  {context.isPending ? <Loading /> : null}
                  {context.error ? <Problem>{context.error.message}</Problem> : null}
                  {ctx && ctx.items.length === 0 ? (
                    <p {...stylex.props(styles.empty)}>
                      No memory yet for this employee. Use Remember, or accept proposals.
                    </p>
                  ) : null}
                  {ctx ? (
                    <>
                      <ContextRows
                        title={`ALWAYS IN CONTEXT · ${place("always").length} · ${thousands(ctx.always_tokens)} TOKENS`}
                        items={place("always")}
                      />
                      <ContextRows
                        title={`ONLY WHEN RELEVANT TO THE QUESTION · ${place("when_relevant").length}`}
                        info="Added only when the question matches and budget is left."
                        items={place("when_relevant")}
                        dim={styles.dim75}
                      />
                      {place("over_budget").length > 5 && !overOpen ? (
                        <div {...stylex.props(styles.expander)}>
                          <Button
                            size="sm"
                            variant="ghost"
                            label={`▸ Does not fit the budget · ${place("over_budget").length}`}
                            onClick={() => setOverOpen(true)}
                          />
                        </div>
                      ) : (
                        <ContextRows
                          title={`DOES NOT FIT THE BUDGET · ${place("over_budget").length}`}
                          info="Not sent; the employee can still find these with recall."
                          items={place("over_budget")}
                          dim={styles.dim55}
                        />
                      )}
                    </>
                  ) : null}
                  <div {...stylex.props(styles.active)}>
                    <Eyebrow>ACTIVE MEMORIES</Eyebrow>
                  </div>
                  {items
                    .filter((m) => m.status === "active")
                    .map((m) => (
                      <Entry key={m.id} m={m} selected={m.id === current?.id} onSelect={() => setChosen(m.id)}>
                        <Button size="sm" label="Revise" onClick={() => setRevising(m)} />
                        <Button size="sm" label={m.pinned ? "Unpin" : "Pin"} onClick={() => void pin(m)} />
                        <Button size="sm" variant="destructive" label="Forget…" onClick={() => setForgetting(m)} />
                      </Entry>
                    ))}
                </div>
              ) : null}
            </div>
          </div>
          <aside aria-label="Remember" {...stylex.props(styles.aside)}>
            <TextArea
              label="Remember for this project"
              rows={4}
              placeholder="For example: publishing requires a prior review"
              value={text}
              onChange={setText}
            />
            <SegmentedControl label="Kind" size="sm" value={kind} onChange={(v) => setKind(v as typeof kind)}>
              <SegmentedControlItem value="project_knowledge" label="Knowledge" />
              <SegmentedControlItem value="decision" label="Decision" />
            </SegmentedControl>
            <div>
              <Button size="sm" variant="primary" label="Remember" isDisabled={text.trim() === ""} onClick={remember} />
            </div>
            <p {...stylex.props(styles.remember)}>{NOTE}</p>
          </aside>
        </div>
      )}
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
    </Page>
  );
}
