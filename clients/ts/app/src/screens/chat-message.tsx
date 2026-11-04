import { expectBody } from "@aurelius/client";
import type { ActivityView, ApprovalView, ContextView, MessageVersionView, MessageView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createContext, memo, useContext, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useRunGuidelines } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { Dialog, Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { ApprovalCard } from "./approval-card";
import type { LiveStore } from "./chat-live";
import { useLive } from "./chat-live";
import {
  applyReaction,
  authorOf,
  type Caps,
  canChange,
  canMakeTicket,
  clock,
  currentStep,
  dayLabel,
  fold,
  isSystemLine,
  lineMark,
  localDay,
  markMentions,
  mentionsMe,
  planTitle,
  prefill,
  REACTIONS,
  reactionOn,
  reactionTip,
  rowActions,
  runLabel,
  saveOf,
  segments,
  splitBlocks,
  systemText,
  threadLabel,
  threadLine,
  ticketFrom,
  todos,
  webSearchSummary,
} from "./chat-logic";
import { messageLink } from "./links-logic";
import { overlays } from "./overlays-store";
import { FileChips, KeptFiles, MarkdownChips, ScreenshotLink, TraceLink, useRetryAnswer } from "./run-tools";
import { stepLine } from "./tasks-logic";

const OUTPUT_LINES = 400;
const TODO_FOLD = 8;

export interface ChatContext {
  store: LiveStore;
  caps: Caps;
  features: string[];
  people: string[];
  me: string | null;
  place: string;
  ticketNumber: number | null;
  roleOf: (name: string) => string | null;
  approvals: Map<string, ApprovalView>;
  openThread: (id: string) => void;
  messages: MessageView[];
  lastAnswer: string | null;
  conversationId: string | null;
  /** The message a link led to, marked for a moment. */
  flash: string | null;
}

const Context = createContext<ChatContext | null>(null);
export const ChatProvider = Context.Provider;
function useChat(): ChatContext {
  const value = useContext(Context);
  if (!value) throw new Error("No chat.");
  return value;
}

const offset = () => -new Date().getTimezoneOffset();
const today = () => localDay(Date.now(), offset());

/** Change one message wherever it is cached: the channel, a thread, a discussion. */
export function usePatchMessage() {
  const client = useQueryClient();
  return (id: string, change: (m: MessageView) => MessageView) => {
    const inList = (list: MessageView[]) => list.map((m) => (m.id === id ? change(m) : m));
    client.setQueriesData<Record<string, unknown>>({ queryKey: ["messages"] }, (old) => {
      if (!old || typeof old !== "object" || Array.isArray(old)) return old;
      const next = { ...old };
      if (Array.isArray(old.messages)) next.messages = inList(old.messages as MessageView[]);
      if (Array.isArray(old.replies)) next.replies = inList(old.replies as MessageView[]);
      if (old.root && (old.root as MessageView).id === id) next.root = change(old.root as MessageView);
      return next;
    });
  };
}

export function DayLine({ ms }: { ms: number }) {
  return (
    <div className="flex items-center gap-3 pt-4 pb-1 text-[12px] font-medium text-text3">
      <span className="h-px flex-1 bg-line2" />
      {dayLabel(localDay(ms, offset()), today())}
      <span className="h-px flex-1 bg-line2" />
    </div>
  );
}

export function NewLine() {
  return (
    <div className="flex items-center gap-3 py-1 text-[12px] font-medium text-amber">
      <span className="h-px flex-1 bg-amber-line" />
      New messages
      <span className="h-px flex-1 bg-amber-line" />
    </div>
  );
}

function ActivityOutput({ step, messageId }: { step: ActivityView; messageId: string }) {
  const connection = useConnection();
  const [full, setFull] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const raw = full ?? step.output ?? "";
  const output = step.tool === "WebSearch" ? webSearchSummary(raw) : raw;
  const lines = output.split("\n");
  const code = stepLine(step);
  const load = async () => {
    setBusy(true);
    try {
      const body = expectBody(
        await connection.call("activity.get", { message_id: messageId, call_id: step.call_id }),
        "activity",
      ).value;
      setFull(body.output ?? "");
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-1 border-t border-line2 px-[10px] py-2">
      <span className="text-[12px] text-text3">{step.tool}</span>
      {code ? <code className="font-mono text-[12px] break-all text-text2">&gt; {code}</code> : null}
      <ScreenshotLink tool={step.tool} output={step.output} className={linkish} />
      {step.tool === "WebSearch" && raw !== output ? (
        <Markdown text={output} />
      ) : output ? (
        <pre className="m-0 max-h-[320px] overflow-auto font-mono text-[12px] whitespace-pre-wrap text-text2">
          {lines.slice(0, OUTPUT_LINES).join("\n")}
          {lines.length > OUTPUT_LINES ? `\n… ${lines.length - OUTPUT_LINES} more lines` : ""}
        </pre>
      ) : null}
      {step.truncated && full === null ? (
        <button
          type="button"
          disabled={busy}
          onClick={() => void load()}
          className="cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-acc-text"
        >
          {busy ? "Loading…" : "Show full output"}
        </button>
      ) : null}
    </div>
  );
}

function StepRun({ steps, messageId }: { steps: ActivityView[]; messageId: string }) {
  const [open, setOpen] = useState(false);
  const last = steps.at(-1) as ActivityView;
  const failed = steps.some((s) => s.ok === false);
  return (
    <div className="overflow-hidden rounded-[6px] border border-line bg-sunk">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex h-8 w-full cursor-pointer items-center gap-[10px] border-0 bg-transparent px-[10px] text-left text-[12px] text-text2"
      >
        <span aria-hidden="true">{open ? "⌄" : "›"}</span>
        <span className={`flex-none font-medium ${failed ? "text-red" : "text-text"}`}>{runLabel(steps)}</span>
        {open ? null : <span className="min-w-0 flex-1 truncate font-mono text-text3">{stepLine(last)}</span>}
      </button>
      {open ? steps.map((s) => <ActivityOutput key={s.call_id} step={s} messageId={messageId} />) : null}
    </div>
  );
}

function Checklist({ activity }: { activity: ActivityView[] }) {
  const [all, setAll] = useState(false);
  const list = todos(activity);
  if (!list) return null;
  const done = list.filter((t) => t.state === "done").length;
  const folded = list.length > TODO_FOLD && !all;
  const shown = folded ? list.filter((t) => t.state === "doing").slice(0, 1) : list;
  return (
    <div className="flex flex-col gap-1 rounded-[6px] border border-line bg-sunk px-3 py-2">
      {shown.map((t) => (
        <div
          key={t.text}
          className={`flex gap-2 ${t.state === "done" ? "text-text3 line-through" : t.state === "doing" ? "font-medium" : "text-text2"}`}
        >
          <span aria-hidden="true">{t.state === "done" ? "✓" : t.state === "doing" ? "▸" : "○"}</span>
          <span className="min-w-0">{t.text}</span>
        </div>
      ))}
      {list.length > TODO_FOLD ? (
        <button
          type="button"
          onClick={() => setAll(!all)}
          className="cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-text3 hover:text-text"
        >
          {folded ? `${done} of ${list.length} done · show all` : "Show less"}
        </button>
      ) : null}
    </div>
  );
}

/** Long text folds; what was seen while it streamed stays open. */
function Text({ text, seenLive }: { text: string; seenLive: boolean }) {
  const { people, me } = useChat();
  const [open, setOpen] = useState(false);
  const folded = useMemo(() => (seenLive ? null : fold(text)), [text, seenLive]);
  const shown = folded && !open ? folded.shown : text;
  const marked = useMemo(() => markMentions(shown, people, me), [shown, people, me]);
  return (
    <>
      <Markdown text={marked} />
      {folded ? (
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-text3 hover:text-text"
        >
          {open ? "▴ Show less" : `▾ Show more (${folded.hidden} lines)`}
        </button>
      ) : null}
    </>
  );
}

/** While it is written each finished block stays put; only the last one changes. */
function LiveText({ text }: { text: string }) {
  const blocks = splitBlocks(text);
  return (
    <>
      {blocks.map((block, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: blocks only ever grow at the end.
        <Markdown key={i} text={block} live />
      ))}
    </>
  );
}

function statusLine(activity: ActivityView[], text: string, waiting: boolean): string {
  if (waiting) return "Waiting for your approval";
  const step = currentStep(activity);
  if (step) return step;
  return text ? "Writing…" : "Thinking…";
}

function AnswerBody({
  m,
  text,
  activity,
  live,
}: {
  m: MessageView;
  text: string;
  activity: ActivityView[];
  live: boolean;
}) {
  const { store } = useChat();
  const parts = useMemo(() => segments(text, activity), [text, activity]);
  return (
    <div className="flex flex-col gap-[10px]">
      <Checklist activity={activity} />
      {parts.map((part, i) =>
        part.kind === "steps" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts only ever grow at the end.
          <StepRun key={i} steps={part.steps} messageId={m.id} />
        ) : live ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts only ever grow at the end.
          <LiveText key={i} text={part.text} />
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts only ever grow at the end.
          <Text key={i} text={part.text} seenLive={store.seen.has(m.id)} />
        ),
      )}
    </div>
  );
}

function StopButton({ run }: { run: string }) {
  const connection = useConnection();
  return (
    <button
      type="button"
      aria-label="Stop"
      title="Stop"
      onClick={() =>
        connection
          .write("run.stop", { run_id: run }, { retry: false })
          .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."))
      }
      className="grid size-5 cursor-pointer place-items-center rounded-[4px] border-0 bg-transparent text-text2 hover:bg-sel hover:text-text"
    >
      ■
    </button>
  );
}

const linkish = "cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-text3 hover:text-text";

function ContextLink({ run }: { run: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)} className={linkish}>
        ≡ Context
      </button>
      {open ? <ContextDialog run={run} onClose={() => setOpen(false)} /> : null}
    </>
  );
}

/** What an answer was given: its full list, with why each part did or didn't go in. */
export function ContextDialog({ run, onClose }: { run: string; onClose: () => void }) {
  const connection = useConnection();
  const guidelines = useRunGuidelines(run);
  const list = guidelines.data?.guidelines ?? [];
  const [view, setView] = useState<ContextView | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    connection
      .call("run.get_context", { run_id: run })
      .then((body) => setView(expectBody(body, "context").value))
      .catch((e: unknown) => setProblem(e instanceof Error ? e.message : "This did not load."));
  }, [connection, run]);
  const sections = view ? [...new Set(view.items.map((i) => i.section))] : [];
  return (
    <Dialog open onClose={onClose} title="Context of this answer" width={720}>
      <div className="flex max-h-[75vh] flex-col gap-3 overflow-y-auto p-4">
        <p className="m-0 text-[15px] font-semibold">Context of this answer</p>
        {view ? (
          <p className="m-0 text-[12px] text-text3">
            {view.runtime_managed
              ? "The runtime manages its own context."
              : `${view.used_tokens} of ${view.budget_tokens} tokens`}
            {view.scopes.length > 0 ? ` · ${view.scopes.join(", ")}` : ""}
          </p>
        ) : null}
        {problem ? (
          <p role="alert" className="m-0 text-red">
            {problem}
          </p>
        ) : null}
        {!view && !problem ? <p className="m-0 text-text3">Loading…</p> : null}
        {sections.map((section) => (
          <section key={section} className="flex flex-col gap-1">
            <p className="m-0 text-[12px] font-medium text-text3">{section}</p>
            {view?.items
              .filter((i) => i.section === section)
              .map((i) => (
                <div key={i.id} className={`flex justify-between gap-3 ${i.included ? "" : "text-text3"}`}>
                  <span className="min-w-0">
                    {i.included ? "" : "Left out: "}
                    {i.label}
                    {i.reason ? <span className="text-text3"> · {i.reason}</span> : null}
                  </span>
                  <span className="flex-none font-mono text-[12px] text-text3">
                    {i.tokens} tok{typeof i.score === "number" ? ` · ${i.score.toFixed(2)}` : ""}
                  </span>
                </div>
              ))}
          </section>
        ))}
        {list.length > 0 ? (
          <section className="flex flex-col gap-1">
            <p className="m-0 text-[12px] font-medium text-text3">Guidelines</p>
            {list.map((g) => (
              <div key={g.guideline_id} className="flex justify-between gap-3">
                <span className="min-w-0 truncate">{g.title}</span>
                <span className="flex-none text-[12px] text-text3">
                  v{g.version} · {g.chars} chars
                </span>
              </div>
            ))}
          </section>
        ) : null}
        <p className="m-0 text-[12px] text-text3">
          Only permitted scopes are searched, before any text is loaded. Memory is data, not instructions.
        </p>
      </div>
    </Dialog>
  );
}

function EditedLink({ id }: { id: string }) {
  const connection = useConnection();
  const [versions, setVersions] = useState<MessageVersionView[] | null>(null);
  const load = async () => {
    try {
      setVersions(
        expectBody(await connection.call("message.history", { message_id: id }), "message_history").value.versions,
      );
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <>
      <button type="button" onClick={() => void load()} className={linkish}>
        edited
      </button>
      {versions ? (
        <Dialog open onClose={() => setVersions(null)} title="Earlier versions" width={560}>
          <div className="flex max-h-[60vh] flex-col gap-3 overflow-y-auto p-4">
            <p className="m-0 text-[12px] font-semibold tracking-[0.04em] text-text3">EARLIER VERSIONS</p>
            {versions.length === 0 ? <p className="m-0 text-text2">The earlier text is gone.</p> : null}
            {versions.map((v) => (
              <div key={v.replaced_at_ms} className="flex flex-col gap-1 border-t border-line2 pt-2">
                <span className="text-[12px] text-text3">{clock(v.replaced_at_ms, offset())}</span>
                <Markdown text={v.text} />
              </div>
            ))}
          </div>
        </Dialog>
      ) : null}
    </>
  );
}

/** An answer in its own row while it streams: only this re-renders. */
function LiveAnswer({ m, waiting }: { m: MessageView; waiting: boolean }) {
  const { store } = useChat();
  const update = useLive(store, m.id);
  const text = update?.text ?? m.text;
  const activity = update?.activity ?? m.activity;
  return (
    <>
      <AnswerBody m={m} text={text} activity={activity} live />
      <div className="flex items-center gap-2 text-[12px] text-acc-text">
        {m.run_id ? <StopButton run={m.run_id} /> : null}

        {update?.progress?.compacting ? "Context comprimeren…" : statusLine(activity, text, waiting)}
      </div>
    </>
  );
}

function Header({
  name,
  kind,
  human,
  at,
  children,
}: {
  name: string;
  kind: string;
  human: boolean;
  at: number;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-baseline gap-2">
      <span className="font-semibold">{name}</span>
      <span className="text-[12px] text-text3">{human ? "· board member" : `— ${kind}`}</span>
      <span className="text-[12px] text-text3">{clock(at, offset())}</span>
      {children}
    </div>
  );
}

function SystemRow({ m }: { m: MessageView }) {
  const { ticketNumber } = useChat();
  const line = m.system;
  if (!line) return <div className="px-10 text-[12px] leading-[1.5] text-text3">{m.text}</div>;
  const text = systemText(line, m.text, ticketNumber);
  const { mark, failed } = lineMark(line, text);
  const [first, reason] = text.split(" Reason:");
  return (
    <section
      className={`flex flex-col gap-[2px] rounded-[8px] border px-3 py-2 text-[13px] ${failed ? "border-red text-red" : "border-line bg-sunk text-text2"}`}
      aria-label={`System: ${text} at ${clock(m.created_at_ms, offset())}`}
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 flex-1">
          {mark}
          {first}
        </span>
        {line.task_id ? (
          <Link
            to="/tasks/$taskId"
            params={{ taskId: line.task_id }}
            className="text-[12px] text-acc-text no-underline"
          >
            Open task →
          </Link>
        ) : line.ticket_id ? (
          <Link to="/board" search={{ ticket: line.ticket_id }} className="text-[12px] text-acc-text no-underline">
            Open ticket →
          </Link>
        ) : null}
      </div>
      {reason ? <span className="line-clamp-2 text-[12px] text-text3">Reason:{reason}</span> : null}
    </section>
  );
}

function UpdateCard({ m }: { m: MessageView }) {
  const { approvals } = useChat();
  const pending = m.approval_id ? approvals.get(m.approval_id) : undefined;
  const isPlan = m.update === "plan";
  const [choice, setChoice] = useState<boolean | null>(null);
  const open = !isPlan || (choice ?? Boolean(pending));
  useAction("TogglePlanFold", () => setChoice(!open), isPlan);
  const label = ((): [string, boolean] => {
    switch (m.update) {
      case "brief":
        return ["TASK", true];
      case "finding":
        return ["FINDING", false];
      case "question":
        return ["NEEDS A DECISION", true];
      case "approval":
        return pending ? ["WAITING FOR YOU", true] : ["ASKED", false];
      case "plan":
        return pending ? ["PLAN · WAITING FOR YOU", true] : ["PLAN", false];
      case "message":
        return ["MESSAGE FROM", false];
      case "mode_chosen":
        return ["MODE CHOSEN", false];
      default:
        return ["UPDATE", false];
    }
  })();
  return (
    <div
      className={`flex flex-col gap-[6px] rounded-[8px] border px-[14px] py-[11px] ${label[1] ? "border-amber-line bg-amber-soft" : "border-line bg-sunk"}`}
    >
      <div className="flex items-center gap-2 text-[11.5px] text-text3">
        <span className="font-semibold tracking-[0.04em]">{label[0]}</span>
        <span className="min-w-0 flex-1 truncate text-text2">{m.author ?? "Aurelius"}</span>
        {isPlan ? (
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setChoice(!open)}
            className="cursor-pointer border-0 bg-transparent p-0 text-text2 hover:text-text"
          >
            {open ? "▾ Hide plan" : "▸ Show plan"}
          </button>
        ) : null}
      </div>
      {open ? (
        <Text text={m.text} seenLive={false} />
      ) : (
        <span className="truncate text-text2">{planTitle(m.text)}</span>
      )}
      {pending ? <ApprovalCard approval={pending} /> : null}
    </div>
  );
}

function Queued({ m }: { m: MessageView }) {
  const connection = useConnection();
  const client = useQueryClient();
  const act = async (method: "conversation.delete_queued" | "conversation.send_queued_now", conversationId: string) => {
    try {
      await connection.write(method, { conversation_id: conversationId, message_id: m.id }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const { conversationId } = useFeedIds();
  return (
    <div className="flex items-center gap-3 text-[12px] text-text3">
      Waiting · sends after the current answer
      {conversationId ? (
        <>
          <button
            type="button"
            onClick={() => void act("conversation.delete_queued", conversationId)}
            className="cursor-pointer border-0 bg-transparent p-0 text-text2 hover:text-text"
          >
            Delete
          </button>
          <button
            type="button"
            onClick={() => void act("conversation.send_queued_now", conversationId)}
            className="cursor-pointer border-0 bg-transparent p-0 text-acc-text"
          >
            Send now
          </button>
        </>
      ) : null}
    </div>
  );
}

const FeedIds = createContext<{ conversationId: string | null }>({ conversationId: null });
export const FeedIdsProvider = FeedIds.Provider;
const useFeedIds = () => useContext(FeedIds);

function Reactions({ m, can }: { m: MessageView; can: boolean }) {
  const list = m.reactions ?? [];
  const react = useReact(m);
  if (list.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1">
      {list.map((r) => (
        <button
          key={r.emoji}
          type="button"
          aria-pressed={r.mine}
          title={reactionTip(r)}
          disabled={!can}
          onClick={() => react(r.emoji)}
          className={`inline-flex h-[22px] cursor-pointer items-center gap-1 rounded-[11px] border px-[7px] text-[12px] ${r.mine ? "border-acc-text bg-sel" : "border-line bg-transparent"}`}
        >
          {r.emoji} <span className="text-text2">{r.count}</span>
        </button>
      ))}
    </div>
  );
}

function useReact(m: MessageView) {
  const connection = useConnection();
  const client = useQueryClient();
  const patch = usePatchMessage();
  const { me } = useChat();
  return (emoji: string) => {
    const on = reactionOn(m.reactions ?? [], emoji);
    patch(m.id, (x) => ({ ...x, reactions: applyReaction(x.reactions ?? [], emoji, on, me) }));
    connection.write("message.react", { message_id: m.id, emoji, on }, { retry: false }).catch((e: unknown) => {
      void client.invalidateQueries({ queryKey: ["messages"] });
      toast(e instanceof Error ? e.message : "This did not work.");
    });
  };
}

function EditBox({ m, onDone }: { m: MessageView; onDone: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [text, setText] = useState(m.text);
  const [gone, setGone] = useState<string | null>(null);
  const platform = usePlatform();
  const save = saveOf(m.text, text);
  const go = async () => {
    if (save.kind === "close") return onDone();
    if (save.kind !== "send") return;
    try {
      await connection.write("message.edit", { message_id: m.id, text: save.text }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages"] });
      onDone();
    } catch (e) {
      setGone(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <div className="flex flex-col gap-2">
      {gone ? (
        <div role="alert" className="flex items-center gap-2 text-[12px] text-red">
          <span className="min-w-0 flex-1">{gone}</span>
          <Button
            onClick={() => {
              void platform.copy(text).then(() => toast("Copied your text."));
              onDone();
            }}
          >
            Copy my text
          </Button>
        </div>
      ) : null}
      <textarea
        aria-label="Edit message"
        ref={(el) => el?.focus({ preventScroll: true })}
        rows={Math.min(10, text.split("\n").length + 1)}
        className="w-full resize-y rounded-[6px] border border-btn-line bg-sunk p-2 text-text outline-none focus:border-acc-text"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void go();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onDone();
          }
        }}
      />
      {save.kind === "off" ? <span className="text-[12px] text-text3">To remove it, delete the message.</span> : null}
      <div className="flex gap-2">
        <Button kind="primary" disabled={save.kind === "off"} onClick={() => void go()}>
          Save
        </Button>
        <Button onClick={onDone}>Cancel</Button>
      </div>
    </div>
  );
}

function RowBar({
  m,
  onEdit,
  onDelete,
  name,
}: {
  m: MessageView;
  onEdit: () => void;
  onDelete: () => void;
  name: string;
}) {
  const platform = usePlatform();
  const { caps, openThread, features, place } = useChat();
  const actions = rowActions(m, caps);
  const react = useReact(m);
  const editable = canChange(m, features.includes("message_edits"));
  const makeTicket = features.includes("ticket_discussion") && canMakeTicket(m);
  const { hostId } = useLinkState();
  const linkable = hostId !== null && features.includes("message_locate") && actions.react && !m.deleted_at_ms;
  const more = [
    { key: "copy", label: "Copy" },
    ...(linkable ? [{ key: "link", label: "Copy link" }] : []),
    ...(makeTicket ? [{ key: "ticket", label: "Create ticket…" }] : []),
    ...(editable
      ? [
          { key: "edit", label: "Edit" },
          { key: "delete", label: "Delete" },
        ]
      : []),
  ];
  const pick = (key: string) => {
    if (key === "copy") void platform.copy(m.text).catch(() => toast("This did not copy."));
    if (key === "link") void platform.copy(messageLink(hostId ?? "", m.id)).then(() => toast("Link copied."));
    if (key === "edit") onEdit();
    if (key === "delete") onDelete();
    if (key === "ticket")
      overlays.open("ticket", { ...prefill(m.text, name, place, clock(m.created_at_ms, offset())), fromMessage: m.id });
  };
  const tool =
    "grid size-[26px] cursor-pointer place-items-center rounded-[5px] border-0 bg-transparent text-text2 hover:bg-sel hover:text-text";
  return (
    <div className="absolute -top-3 right-2 hidden gap-[1px] rounded-[7px] border border-line bg-panel p-[2px] shadow-card group-focus-within/row:flex group-hover/row:flex has-data-[state=open]:flex">
      {actions.react ? (
        <Menu
          label="React"
          options={REACTIONS.map((e) => ({ key: e, label: reactionOn(m.reactions ?? [], e) ? e : `${e}  Take back` }))}
          onPick={react}
          trigger={
            <button type="button" aria-label="React" className={tool}>
              ☺+
            </button>
          }
        />
      ) : null}
      {actions.reply ? (
        <button type="button" aria-label="Reply in thread" onClick={() => openThread(m.id)} className={tool}>
          ↳
        </button>
      ) : null}
      <Menu
        label="More actions"
        options={more}
        onPick={pick}
        trigger={
          <button type="button" aria-label="More actions" className={tool}>
            ⋯
          </button>
        }
      />
    </div>
  );
}

function ThreadLink({ m }: { m: MessageView }) {
  const { openThread } = useChat();
  const line = threadLine(m, offset(), today());
  if (!line) return null;
  return (
    <button
      type="button"
      aria-label={threadLabel(m, offset(), today())}
      onClick={() => openThread(m.id)}
      className="flex h-7 cursor-pointer items-center gap-2 self-start rounded-[6px] border-0 bg-transparent px-1 text-[12px] text-acc-text hover:bg-sel"
    >
      <span className="flex">
        {(m.thread_repliers ?? []).slice(0, 3).map((name) => (
          <span key={name} className="-ml-1 first:ml-0">
            <Mark letter={initial(name)} color={markColor(name)} size={18} radius={5} />
          </span>
        ))}
      </span>
      {line}
      {(m.thread_unread ?? 0) > 0 ? <span className="font-semibold text-amber">{m.thread_unread} new</span> : null}
    </button>
  );
}

export const MessageRow = memo(function MessageRow({
  m,
  grouped,
  inThread = false,
}: {
  m: MessageView;
  grouped: boolean;
  inThread?: boolean;
}) {
  const { roleOf, caps, approvals, me, messages, lastAnswer, conversationId } = useChat();
  const made = useMemo(() => ticketFrom(messages, m.id), [messages, m.id]);
  const { flash } = useChat();
  const connection = useConnection();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const retry = useRetryAnswer(conversationId, messages);
  if (m.system || isSystemLine(m, true)) return <SystemRow m={m} />;
  const who = authorOf(m, roleOf);
  const body = m.deleted_at_ms ? (
    <span className="text-text3">⊘ This message was deleted.</span>
  ) : editing ? (
    <EditBox m={m} onDone={() => setEditing(false)} />
  ) : m.update && m.update !== "group" ? (
    <UpdateCard m={m} />
  ) : m.status === "partial" ? (
    <LiveAnswer m={m} waiting={[...approvals.values()].some((a) => a.run_id === m.run_id)} />
  ) : (
    <>
      <AnswerBody m={m} text={m.text} activity={m.activity} live={false} />
      <MarkdownChips
        text={m.text}
        scope={conversationId ? { kind: "conversation", conversation_id: conversationId } : null}
      />
      <FileChips files={m.uploads} />
      <KeptFiles files={m.files ?? []} />
      <KeptFiles files={m.files ?? []} />
      {m.id === lastAnswer && m.status === "complete" && m.run_id ? (
        <span className="flex gap-3">
          <ContextLink run={m.run_id} />
          <TraceLink run={m.run_id} className={linkish} />
        </span>
      ) : null}
      {m.status === "interrupted" ? (
        <span className="text-[12px] text-text3">Stopped; text up to that point is kept.</span>
      ) : null}
      {m.status === "failed" ? (
        <span className="text-[12px] text-red">Failed; this doesn't count as an answer.</span>
      ) : null}
      {m.id === lastAnswer && (m.status === "failed" || m.status === "interrupted") ? (
        <button type="button" onClick={retry} className={linkish}>
          ↻ Retry answer
        </button>
      ) : null}
    </>
  );
  const remove = async () => {
    try {
      await connection.write("message.delete", { message_id: m.id }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <div
      className={`group/row relative flex gap-3 rounded-[6px] hover:bg-[color-mix(in_srgb,var(--sel)_45%,transparent)] ${grouped ? "pt-[2px]" : "pt-2"} ${flash === m.id ? "bg-sel" : ""}`}
    >
      <div className="w-7 flex-none">
        {grouped ? null : (
          <Mark
            letter={initial(who.name)}
            color={who.human ? "#55595F" : markColor(who.name)}
            size={28}
            radius={who.human ? 14 : 7}
          />
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        {grouped ? null : (
          <Header name={who.name} kind={who.kind} human={who.human} at={m.created_at_ms}>
            {mentionsMe(m.text, me) && m.role !== "user" ? (
              <span className="text-[12px] font-medium text-amber">@ you</span>
            ) : null}
            {m.edited_at_ms && !m.deleted_at_ms ? <EditedLink id={m.id} /> : null}
            {made ? (
              made.ticket ? (
                <Link
                  to="/board"
                  search={{ ticket: made.ticket }}
                  className="rounded-[9px] border border-line px-[6px] text-[12px] text-acc-text no-underline"
                >
                  → {made.label}
                </Link>
              ) : (
                <span className="text-[12px] text-text3">→ {made.label}</span>
              )
            ) : null}
          </Header>
        )}
        {body}
        {m.queued ? <Queued m={m} /> : null}
        <Reactions m={m} can={!caps.readOnly} />
        {inThread ? null : <ThreadLink m={m} />}
      </div>
      {editing || m.deleted_at_ms || m.status === "partial" ? null : (
        <RowBar m={m} name={who.name} onEdit={() => setEditing(true)} onDelete={() => setDeleting(true)} />
      )}
      {deleting ? (
        <Dialog open onClose={() => setDeleting(false)} title="Delete this message?" width={440}>
          <div className="flex flex-col gap-2 p-4">
            <p className="m-0 text-[15px] font-semibold">Delete this message?</p>
            <p className="m-0 text-text2">The channel keeps a line saying a message was deleted.</p>
          </div>
          <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
            <Button onClick={() => setDeleting(false)}>Cancel</Button>
            <Button
              data-autofocus
              kind="danger"
              onClick={() => {
                setDeleting(false);
                void remove();
              }}
            >
              Delete
            </Button>
          </div>
        </Dialog>
      ) : null}
    </div>
  );
});

/** An answer the list does not hold yet. */
export function LiveRow({ id, name, kind }: { id: string; name: string; kind: string }) {
  const { store } = useChat();
  const update = useLive(store, id);
  if (!update) return null;
  const stub: MessageView = {
    id,
    activity: [],
    created_at_ms: Date.now(),
    files: [],
    ordinal: 0,
    queued: false,
    role: "assistant",
    sources: [],
    status: "partial",
    text: "",
    uploads: [],
  };
  return (
    <div className="flex gap-3 pt-2">
      <div className="w-7 flex-none">
        <Mark letter={initial(name)} color={markColor(name)} size={28} radius={7} />
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex items-baseline gap-2">
          <span className="font-semibold">{name}</span>
          <span className="text-[12px] text-text3">— {kind}</span>
        </div>
        <LiveAnswer m={stub} waiting={false} />
      </div>
    </div>
  );
}
