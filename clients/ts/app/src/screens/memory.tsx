import type { MemoryContextItem, MemoryView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { useConnection } from "../connection";
import { useEmployees, useMemory, useMemoryContext, useProjects } from "../data";
import { useAction } from "../keys/provider";
import { Button, Segments } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { toast } from "../ui/toast";
import { consolidationScopes, memoryHistory, memoryKind, memoryProposals, thousands } from "./knowledge-logic";
import { AREA, Confirm, EYEBROW, failureOf, hhmm, KnowledgeTabs, PageHead, SMALL } from "./org-kit";

type Tab = "context" | "proposals" | "history";

const NOTE =
  "Employees propose memories during runs. Proposals wait here until you accept or reject them; accepted ones go into every run in this project.";

function Entry({ m, children }: { m: MemoryView; children?: React.ReactNode }) {
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
    <article className="flex flex-col gap-2 border-b border-line2 px-[18px] py-4">
      <div className="flex flex-wrap items-center gap-2 text-[12px] text-text3">
        <span className="inline-flex h-5 items-center gap-[6px] rounded-[10px] border border-line px-[7px] text-[11px] text-text2">
          <span className="size-[6px] rounded-full bg-violet" />
          {memoryKind(m)}
        </span>
        {badge ? (
          <span
            className={`inline-flex h-5 items-center rounded-[4px] bg-sel px-[6px] text-[11px] tracking-[0.04em] ${m.status === "proposed" ? "text-acc-text" : "text-text3"}`}
          >
            {badge}
          </span>
        ) : null}
        {m.scope === "global" ? <span>· global</span> : m.employee_name ? <span>· {m.employee_name}</span> : null}
        {m.pinned ? <span>· pinned</span> : null}
        <span className="ml-auto">{source ? `source: ${source.label} · ${hhmm(source.at_ms)}` : "no source"}</span>
      </div>
      <p className="m-0 text-[14px] leading-[1.6] whitespace-pre-wrap">
        {m.content || <span className="text-text3">(content erased)</span>}
      </p>
      {children ? <div className="flex flex-wrap gap-2">{children}</div> : null}
    </article>
  );
}

function ReviseDialog({ m, onClose }: { m: MemoryView; onClose: () => void }) {
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
    <Dialog open onClose={onClose} title="Revise memory" width={520}>
      <div className="flex flex-col gap-2 p-4">
        <p className="m-0 text-[15px] font-semibold">Revise memory</p>
        <p className="m-0 text-[12px] text-text3">
          The old version stays visible in the history but is no longer used.
        </p>
        <textarea
          data-autofocus
          aria-label="Memory"
          rows={5}
          className={AREA}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={text.trim() === ""} onClick={() => void save()}>
          Save new version
        </Button>
      </div>
    </Dialog>
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
  dim: string;
}) {
  if (items.length === 0) return null;
  return (
    <section className="flex flex-col gap-1 px-[18px] py-3">
      <p className={EYEBROW} title={info}>
        {title}
      </p>
      {info ? <p className="m-0 text-[12px] text-text3">{info}</p> : null}
      {items.map((i) => (
        <div key={i.memory_id} className={`flex items-start gap-3 border-b border-line2 py-2 ${dim}`}>
          <span className="min-w-0 flex-1 whitespace-pre-wrap">{i.content}</span>
          <span className="flex-none text-[12px] text-text3">
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
  const [tab, setTab] = useState<Tab>("proposals");
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
  useAction("Filter1", () => setTab("context"));
  useAction("Filter2", () => setTab("proposals"));
  useAction("Filter3", () => setTab("history"));
  const ctx = context.data;
  const place = (p: string) => (ctx?.items ?? []).filter((i) => i.placement === p);
  const budget = Math.max(1, ctx?.memory_budget_tokens ?? 1);
  const over = (ctx?.always_tokens ?? 0) > budget;
  const tabs: { key: Tab; label: string; n?: number }[] = [
    { key: "context", label: "In context" },
    { key: "proposals", label: "Proposals", n: proposals.length },
    { key: "history", label: "History", n: history.length },
  ];
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {chrome ? (
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
          <select
            aria-label="Memory of"
            value={project ?? ""}
            onChange={(e) => {
              setPicked(e.target.value);
              setWho(undefined);
            }}
            className="h-6 rounded-[12px] border border-btn-line bg-transparent px-2 text-[12px] text-text outline-none"
          >
            {(projects.data ?? [])
              .filter((p) => p.status !== "archived")
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
          </select>
          <Button className={SMALL} disabled={!project} onClick={() => void consolidate()}>
            Consolidate
          </Button>
        </PageHead>
      ) : null}
      {!project && projects.data ? (
        <p className="m-0 p-6 text-text3">Make a project first: memory belongs to a project.</p>
      ) : (
        <div className="flex min-h-0 flex-1">
          <div className="flex min-w-0 flex-[999_1_520px] flex-col">
            <div role="tablist" aria-label="Memory" className="flex gap-[18px] border-b border-line2 px-[18px]">
              {tabs.map((t) => (
                <button
                  key={t.key}
                  type="button"
                  role="tab"
                  aria-selected={tab === t.key}
                  onClick={() => setTab(t.key)}
                  className={`-mb-px h-[38px] cursor-pointer border-0 border-b-2 bg-transparent px-0 text-[13px] font-medium ${tab === t.key ? "border-text text-text" : "border-transparent text-text2"}`}
                >
                  {t.label} {t.n ? <span className="text-text3">{t.n}</span> : null}
                </button>
              ))}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {memory.isPending ? <p className="m-0 p-6 text-text3">Loading memory…</p> : null}
              {tab === "proposals" && memory.data ? (
                proposals.length === 0 ? (
                  <p className="m-0 p-6 text-text3">No proposals.</p>
                ) : (
                  proposals.map((m) => (
                    <Entry key={m.id} m={m}>
                      <Button kind="primary" className={SMALL} onClick={() => void decide(m, true)}>
                        Accept
                      </Button>
                      <Button className={SMALL} onClick={() => void decide(m, false)}>
                        Reject
                      </Button>
                    </Entry>
                  ))
                )
              ) : null}
              {tab === "history" && memory.data ? (
                history.length === 0 ? (
                  <p className="m-0 p-6 text-text3">Nothing superseded, rejected or forgotten yet.</p>
                ) : (
                  history.map((m) => <Entry key={m.id} m={m} />)
                )
              ) : null}
              {tab === "context" ? (
                <div className="flex flex-col">
                  <div className="flex flex-wrap items-center gap-3 border-b border-line2 px-[18px] py-3 text-[12px] text-text3">
                    Context of
                    <select
                      aria-label="Context of"
                      value={contextEmployee ?? ""}
                      onChange={(e) => setWho(e.target.value || null)}
                      className="h-6 rounded-[6px] border border-btn-line bg-transparent px-2 text-text outline-none"
                    >
                      {staff.map((e) => (
                        <option key={e.id} value={e.id}>
                          {e.name}
                        </option>
                      ))}
                      <option value="">No employee</option>
                    </select>
                    {ctx ? (
                      <>
                        <span>
                          {ctx.system_tokens ? `Instructions ~${thousands(ctx.system_tokens)} tokens · ` : ""}Memory{" "}
                          {thousands(ctx.always_tokens)} / {thousands(ctx.memory_budget_tokens)} tokens
                        </span>
                        <span className="inline-block h-1 w-[320px] max-w-full overflow-hidden rounded-[2px] bg-sel">
                          <span
                            className={`block h-full ${over ? "bg-red" : "bg-acc"}`}
                            style={{ width: `${Math.min(100, (ctx.always_tokens / budget) * 100)}%` }}
                          />
                        </span>
                      </>
                    ) : null}
                  </div>
                  {context.isPending ? <p className="m-0 p-6 text-text3">Loading what goes into the context…</p> : null}
                  {context.error ? (
                    <p role="alert" className="m-0 p-6 text-red">
                      {context.error.message}
                    </p>
                  ) : null}
                  {ctx && ctx.items.length === 0 ? (
                    <p className="m-0 p-6 text-text3">
                      No memory yet for this employee. Use Remember, or accept proposals.
                    </p>
                  ) : null}
                  {ctx ? (
                    <>
                      <ContextRows
                        title={`ALWAYS IN CONTEXT · ${place("always").length} · ${thousands(ctx.always_tokens)} TOKENS`}
                        items={place("always")}
                        dim=""
                      />
                      <ContextRows
                        title={`ONLY WHEN RELEVANT TO THE QUESTION · ${place("when_relevant").length}`}
                        info="Added only when the question matches and budget is left."
                        items={place("when_relevant")}
                        dim="opacity-75"
                      />
                      {place("over_budget").length > 5 && !overOpen ? (
                        <button
                          type="button"
                          onClick={() => setOverOpen(true)}
                          className="cursor-pointer border-0 bg-transparent px-[18px] py-2 text-left text-[12px] text-text3 hover:text-text"
                        >
                          ▸ Does not fit the budget · {place("over_budget").length}
                        </button>
                      ) : (
                        <ContextRows
                          title={`DOES NOT FIT THE BUDGET · ${place("over_budget").length}`}
                          info="Not sent; the employee can still find these with recall."
                          items={place("over_budget")}
                          dim="opacity-55"
                        />
                      )}
                    </>
                  ) : null}
                  <p className={`${EYEBROW} px-[18px] pt-3`}>ACTIVE MEMORIES</p>
                  {items
                    .filter((m) => m.status === "active")
                    .map((m) => (
                      <Entry key={m.id} m={m}>
                        <Button className={SMALL} onClick={() => setRevising(m)}>
                          Revise
                        </Button>
                        <Button
                          className={SMALL}
                          onClick={() =>
                            void act(
                              () =>
                                connection.write(
                                  "memory.pin",
                                  { memory_id: m.id, pinned: !m.pinned },
                                  { retry: false },
                                ),
                              m.pinned ? "Unpinned." : "Pinned: always included as context.",
                            )
                          }
                        >
                          {m.pinned ? "Unpin" : "Pin"}
                        </Button>
                        <Button kind="danger" className={SMALL} onClick={() => setForgetting(m)}>
                          Forget…
                        </Button>
                      </Entry>
                    ))}
                </div>
              ) : null}
            </div>
          </div>
          <aside
            aria-label="Remember"
            className="flex min-w-0 flex-[1_1_320px] flex-col gap-[14px] overflow-y-auto border-l border-line bg-sunk p-[18px]"
          >
            <label className="flex flex-col gap-1 text-[12px] text-text3">
              Remember for this project
              <textarea
                rows={4}
                placeholder="For example: publishing requires a prior review"
                className={AREA}
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
            </label>
            <Segments
              label="Kind"
              value={kind}
              options={[
                { key: "project_knowledge", label: "Knowledge" },
                { key: "decision", label: "Decision" },
              ]}
              onChange={setKind}
            />
            <Button kind="primary" className={`${SMALL} self-start`} disabled={text.trim() === ""} onClick={remember}>
              Remember
            </Button>
            <p className="m-0 border-t border-line2 pt-[14px] text-[12px] leading-[1.6] text-text3">{NOTE}</p>
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
    </div>
  );
}
