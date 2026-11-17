// What the desktop app shows around a run: its model, its cost, its trace, its helpers, and the
// files it refers to.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Dialog } from "@astryxdesign/core/Dialog";
import { List, ListItem } from "@astryxdesign/core/List";
import { Table } from "@astryxdesign/core/Table";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type {
  AgentView,
  DirEntryView,
  FileRef,
  FileScope,
  MessageFileView,
  MessageView,
  TaskRun,
  TokenUsage,
} from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Markdown } from "../ui/markdown";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { FormDialog, Hint, Loading, Problem } from "./org-kit";
import {
  elapsedText,
  markdownPaths,
  screenshotId,
  subagentDoingText,
  subagentKey,
  traceSessionText,
  usageLine,
  waitingText,
} from "./run-logic";
import { modelName } from "./settings-kit";
import { costText, tokensShort } from "./usage-logic";

const styles = stylex.create({
  usage: {
    flexShrink: 0,
    borderRadius: 5,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 8,
    paddingBlock: 2,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  runMenu: {
    height: 24,
    cursor: "pointer",
    borderRadius: 5,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 8,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  linkish: {
    cursor: "pointer",
    alignSelf: "flex-start",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  line: { margin: 0, fontSize: 12, color: "var(--color-text-secondary)" },
  summary: { cursor: "pointer", fontSize: 12, fontWeight: 500, color: "var(--color-text-secondary)" },
  traceText: {
    marginTop: 8,
    marginBottom: 0,
    maxHeight: 300,
    overflow: "auto",
    borderRadius: 6,
    backgroundColor: "var(--precision-sunk)",
    padding: 8,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "pre-wrap",
  },
  side: {
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingTop: 16,
  },
  sideTitle: { marginTop: 0, marginBottom: 8, fontSize: 12, fontWeight: 500, color: "var(--precision-text3)" },
  agent: { display: "flex", flexDirection: "column", paddingBlock: 4, fontSize: 12 },
  agentButton: {
    width: "100%",
    cursor: "pointer",
    borderWidth: 0,
    borderRadius: 5,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 4,
    marginInline: -4,
    textAlign: "start",
    color: "inherit",
    font: "inherit",
  },
  promptText: { marginTop: 0 },
  agentHead: { display: "flex", justifyContent: "space-between", gap: 8 },
  agentName: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", fontWeight: 500 },
  muted: { flexShrink: 0, color: "var(--precision-text3)" },
  agentDoing: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  shot: { display: "flex", maxHeight: "80vh", justifyContent: "center", overflow: "auto", padding: 16 },
  shotImage: { maxWidth: "100%", borderRadius: 6 },
  loadingLine: { margin: 0, color: "var(--precision-text3)" },
  chips: { display: "flex", flexWrap: "wrap", gap: 6, paddingTop: 2 },
  keptChips: { gap: 8 },
  chip: {
    display: "flex",
    cursor: "pointer",
    alignItems: "center",
    gap: 6,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    backgroundColor: "transparent",
    paddingInline: 9,
    paddingBlock: 4,
    fontSize: 12,
    color: "var(--color-text-primary)",
  },
  kept: { borderRadius: 7, paddingInline: 10, paddingBlock: 6, fontSize: 12.5, gap: 8 },
  accent: { color: "var(--color-text-accent)" },
  size: { fontSize: 11, color: "var(--precision-text3)" },
  thumbButton: { cursor: "pointer", borderWidth: 0, backgroundColor: "transparent", padding: 0 },
  thumb: { display: "block", height: 56, maxWidth: 96, borderRadius: 6, objectFit: "cover" },
  uploaded: { alignItems: "center" },
  fullImage: { display: "block", maxHeight: "80vh", width: "100%", objectFit: "contain" },
  folderPath: {
    margin: 0,
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  folders: {
    height: 280,
    overflowY: "auto",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
  },
  footerStart: { marginInlineEnd: "auto" },
});

/** A picture and nothing else; its name is what a screen reader hears. */
function ImageDialog({
  title,
  width,
  onClose,
  children,
}: {
  title: string;
  width: number;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <Dialog
      isOpen
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      aria-label={title}
      width={width}
      maxHeight="calc(100vh - 48px)"
      purpose="form"
      padding={0}
    >
      {children}
    </Dialog>
  );
}

const all = (u: TokenUsage) => u.input_tokens + u.output_tokens + u.cached_tokens;

/** The whole conversation's tokens, with what its subtasks used. */
export function UsageChip({ conversationId, taskId }: { conversationId?: string | null; taskId?: string | null }) {
  const connection = useConnection();
  const totals = useKeyed(
    ["usage", "totals", conversationId ?? null, taskId ?? null],
    async () =>
      expectBody(
        await connection.call("usage.get_totals", { conversation_id: conversationId ?? null, task_id: taskId ?? null }),
        "usage_totals",
      ).value,
    Boolean(conversationId || taskId),
  );
  const t = totals.data;
  if (!t || all(t.total) === 0) return null;
  const cost = costText(t.total.cost_micro_usd, t.total.estimate);
  const tip = [
    `This conversation: ${usageLine(t.own)}`,
    ...t.subtasks.map((s) => `${s.title}${s.employee_name ? ` (${s.employee_name})` : ""}: ${usageLine(s.usage)}`),
    ...(t.subtasks.length > 0 ? [`Total: ${usageLine(t.total)}`] : []),
  ].join("\n");
  const subtasks = t.subtasks.length;
  return (
    <span title={tip} {...stylex.props(styles.usage)}>
      {tokensShort(all(t.total))} tokens · {cost}
      {subtasks > 0 ? ` · ${subtasks} subtask${subtasks === 1 ? "" : "s"}` : ""}
    </span>
  );
}

const EFFORTS = ["low", "medium", "high", "xhigh", "max"];

/** Ctrl+M: what this conversation runs on, from its next run. */
export function RunMenu({ conversationId, run, models }: { conversationId: string; run: TaskRun; models: string[] }) {
  const connection = useConnection();
  const client = useQueryClient();
  const set = async (next: { model: string | null; effort: string | null }) => {
    try {
      await connection.write("conversation.set_run", { conversation_id: conversationId, ...next }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages", conversationId] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const [open, setOpen] = useState(false);
  useAction("CycleModel", () => setOpen(true));
  const current = run.model ?? null;
  const effort = run.effort ?? null;
  const label = `${current ? modelName(current) : "Employee default"}${effort ? ` · ${effort}` : ""}`;
  const options = [
    { key: "default", label: `${current || effort ? "" : "✓ "}Employee default` },
    ...models.map((m) => ({ key: `m:${m}`, label: `${m === current ? "✓ " : ""}${modelName(m)}` })),
    ...EFFORTS.map((e) => ({ key: `e:${e}`, label: `${e === effort ? "✓ " : ""}Effort ${e}` })),
  ];
  return (
    <Menu
      label="Model"
      open={open}
      onOpenChange={setOpen}
      options={options}
      onPick={(key) => {
        if (key === "default") void set({ model: null, effort: null });
        else if (key.startsWith("m:")) void set({ model: key.slice(2), effort });
        else void set({ model: current, effort: key.slice(2) });
      }}
      trigger={
        <button type="button" {...stylex.props(styles.runMenu)}>
          {label}
        </button>
      }
    />
  );
}

/** The whole composed prompt of a run; the server only has it while trace mode is on. */
export function TraceLink({ run }: { run: string }) {
  const connection = useConnection();
  const on = useKeyed(
    ["settings", "trace"],
    async () => expectBody(await connection.call("trace.get_setting"), "trace_setting").value,
  );
  const [open, setOpen] = useState(false);
  if (!on.data?.enabled) return null;
  return (
    <>
      <button type="button" onClick={() => setOpen(true)} {...stylex.props(styles.linkish)}>
        ⌕ Trace
      </button>
      {open ? <TraceDialog run={run} onClose={() => setOpen(false)} /> : null}
    </>
  );
}

function TraceDialog({ run, onClose }: { run: string; onClose: () => void }) {
  const connection = useConnection();
  const trace = useKeyed(
    ["settings", "trace", run],
    async () => expectBody(await connection.call("run.get_trace", { run_id: run }), "run_trace").value,
  );
  const t = trace.data;
  return (
    <FormDialog title="Trace" width={720} onClose={onClose} footer={<Button label="Close" onClick={onClose} />}>
      {trace.isPending ? <Loading /> : null}
      {trace.error ? <Hint>{trace.error.message}</Hint> : null}
      {t ? (
        <>
          <p {...stylex.props(styles.line)}>
            {[
              `Model ${t.model}`,
              t.effort ? `effort ${t.effort}` : null,
              t.mode ? `mode ${t.mode}` : null,
              `tools ${t.profile}`,
              traceSessionText(t.session),
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
          {t.usage ? <p {...stylex.props(styles.line)}>{usageLine(t.usage)}</p> : null}
          <Table
            density="compact"
            idKey="id"
            columns={[
              { key: "part", header: "Part" },
              { key: "chars", header: "Chars", align: "end" },
              { key: "tokens", header: "≈ tokens", align: "end" },
            ]}
            data={t.parts.map((p) => ({
              id: `${p.area}|${p.label}`,
              part: `${p.area} · ${p.label}`,
              chars: p.chars,
              tokens: p.tokens,
            }))}
          />
          {t.steps.length > 0 ? (
            <Table
              density="compact"
              idKey="step"
              columns={[
                { key: "step", header: "Step" },
                { key: "input", header: "in", align: "end" },
                { key: "written", header: "cache write", align: "end" },
                { key: "read", header: "cache read", align: "end" },
                { key: "output", header: "out", align: "end" },
                { key: "tools", header: "tools asked" },
              ]}
              data={t.steps.map((s, i) => ({
                step: i + 1,
                input: tokensShort(s.input_tokens),
                written: tokensShort(s.cache_creation_tokens),
                read: tokensShort(s.cache_read_tokens),
                output: tokensShort(s.output_tokens),
                tools: `${s.subagent ? "subagent · " : ""}${s.tools.join(", ")}`,
              }))}
            />
          ) : null}
          <TraceText title="System prompt" text={t.system_text} />
          <TraceText title="This turn" text={t.user_turn} />
          <Hint>Token counts per part are estimates (4 characters ≈ 1 token).</Hint>
        </>
      ) : null}
    </FormDialog>
  );
}

function TraceText({ title, text }: { title: string; text: string }) {
  return (
    <details>
      <summary {...stylex.props(styles.summary)}>{title}</summary>
      <pre {...stylex.props(styles.traceText)}>{text}</pre>
    </details>
  );
}

/**
 * The helpers of a running answer; nothing tells when they change, so it looks while the run lasts.
 * A row opens what the subagent was asked and its latest step, which follow along while it runs.
 */
export function SubagentsAtWork({ conversationId, live }: { conversationId: string; live: boolean }) {
  const connection = useConnection();
  const [agents, setAgents] = useState<AgentView[]>([]);
  const [opened, setOpened] = useState<AgentView | null>(null);
  useEffect(() => {
    if (!live) {
      setAgents([]);
      return;
    }
    const read = () =>
      connection
        .call("subagent.list", { conversation_id: conversationId })
        .then((body) => setAgents(expectBody(body, "subagents").value))
        .catch(() => setAgents([]));
    void read();
    const tick = setInterval(read, 2000);
    return () => clearInterval(tick);
  }, [connection, conversationId, live]);
  const now = Date.now();
  // The open one as the latest list tells it; once it is done, the last that was seen.
  const shown = opened ? (agents.find((a) => subagentKey(a) === subagentKey(opened)) ?? null) : null;
  const dialog = opened ? (
    <SubagentDialog agent={shown ?? opened} done={shown === null} now={now} onClose={() => setOpened(null)} />
  ) : null;
  if (agents.length === 0) return dialog;
  return (
    <div {...stylex.props(styles.side)}>
      <p {...stylex.props(styles.sideTitle)}>Subagents at work</p>
      {agents.map((a) => (
        <button
          key={subagentKey(a)}
          type="button"
          title="Show what this subagent was asked and what it is doing"
          onClick={() => setOpened(a)}
          {...stylex.props(styles.agent, styles.agentButton)}
        >
          <span {...stylex.props(styles.agentHead)}>
            <span {...stylex.props(styles.agentName)}>{a.name}</span>
            <span {...stylex.props(styles.muted)}>{elapsedText(now - a.since_ms)}</span>
          </span>
          <span {...stylex.props(styles.agentDoing)}>{a.doing ?? a.detail}</span>
        </button>
      ))}
      {dialog}
    </div>
  );
}

/** One subagent: its type, how long it runs, its latest step and what it was asked. */
function SubagentDialog({
  agent,
  done,
  now,
  onClose,
}: {
  agent: AgentView;
  done: boolean;
  now: number;
  onClose: () => void;
}) {
  const prompt = agent.prompt?.trim() ?? "";
  return (
    <FormDialog title={agent.name} width={640} onClose={onClose} footer={<Button label="Close" onClick={onClose} />}>
      <p {...stylex.props(styles.line)}>
        {[agent.detail || "Subagent", done ? "done" : `running ${elapsedText(now - agent.since_ms)}`].join(" · ")}
      </p>
      <p {...stylex.props(styles.line)}>
        {done ? `Last step: ${subagentDoingText(agent)}` : `Now: ${subagentDoingText(agent)}`}
      </p>
      {prompt ? (
        <>
          <p {...stylex.props(styles.sideTitle)}>What it was asked</p>
          <pre {...stylex.props(styles.traceText, styles.promptText)}>{prompt}</pre>
        </>
      ) : null}
    </FormDialog>
  );
}

/** Where a message goes again: an employee's conversation, or the Team chat. */
export type RetryTarget = { employeeId: string | null } | { channel: string | null; threadParent?: string | null };

/**
 * Asks the last question again. The server only retries in conversations of neither employee nor
 * team, so here the question goes again, as a failed answer says to do. A task that never started
 * has only its assignment, which the server wrote: a message starts it.
 */
export function useRetryAnswer(
  conversationId: string | null | undefined,
  messages: MessageView[],
  target: RetryTarget,
) {
  const connection = useConnection();
  const last = [...messages].reverse().find((m) => m.role === "user");
  return () => {
    if (!conversationId || messages.length === 0) return toast("Open a chat with a question first.");
    const text = last?.text ?? "Please try again.";
    const sent =
      "channel" in target
        ? connection.write(
            "group_chat.send",
            { text, channel: target.channel, thread_parent: target.threadParent ?? null },
            { retry: false },
          )
        : target.employeeId
          ? connection.write(
              "employee.send_message",
              { employee_id: target.employeeId, conversation_id: conversationId, text },
              { retry: false },
            )
          : last
            ? connection.write(
                "chat.retry",
                { conversation_id: conversationId, user_message_id: last.id },
                { retry: false },
              )
            : connection.write("chat.send", { conversation_id: conversationId, text }, { retry: false });
    sent.catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  };
}

/** Under a chat or task that waits to start because the computer is busy. */
export function LoadBanner({ waiting }: { waiting: boolean }) {
  const connection = useConnection();
  const client = useQueryClient();
  const load = useKeyed(
    ["settings", "load"],
    async () => expectBody(await connection.call("load.get"), "load").value,
    waiting,
  );
  useEffect(
    () => connection.loads.on((snapshot) => client.setQueryData(["settings", "load"], snapshot)),
    [connection, client],
  );
  const text = waiting && load.data && load.data.held > 0 ? waitingText(load.data) : null;
  if (!text) return null;
  return <Banner status="warning" role="status" title={text} />;
}

function ScreenshotDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const connection = useConnection();
  const shot = useKeyed(
    ["screenshot", id],
    async () => expectBody(await connection.call("screenshot.get", { id }), "screenshot").value,
  );
  return (
    <ImageDialog title="Screenshot" width={960} onClose={onClose}>
      <div {...stylex.props(styles.shot)}>
        {shot.data ? (
          <img
            alt="What the employee saw"
            {...stylex.props(styles.shotImage)}
            src={`data:image/jpeg;base64,${shot.data.jpeg_base64}`}
          />
        ) : (
          <p {...stylex.props(styles.loadingLine)}>{shot.error ? "This screenshot is no longer there." : "Loading…"}</p>
        )}
      </div>
    </ImageDialog>
  );
}

/** A screenshot step's link; other steps have none. */
export function ScreenshotLink({ tool, output }: { tool: string; output?: string | null }) {
  const [open, setOpen] = useState(false);
  const id = tool === "mcp__aurelius__screenshot" ? screenshotId(output ?? "") : null;
  if (!id) return null;
  return (
    <>
      <button type="button" {...stylex.props(styles.linkish)} onClick={() => setOpen(true)}>
        View screenshot
      </button>
      {open ? <ScreenshotDialog id={id} onClose={() => setOpen(false)} /> : null}
    </>
  );
}

function MarkdownDialog({ path, scope, onClose }: { path: string; scope: FileScope; onClose: () => void }) {
  const connection = useConnection();
  const platform = usePlatform();
  const file = useKeyed(
    ["markdown", path, scope],
    async () => expectBody(await connection.call("markdown.get", { path, scope }), "markdown_file").value,
  );
  return (
    <FormDialog
      title={path}
      width={760}
      onClose={onClose}
      footer={
        <>
          {file.data ? (
            <Button
              label="Copy raw"
              onClick={() => {
                void platform.copy(file.data.text).then(() => toast("Copied the Markdown"));
              }}
            />
          ) : null}
          <Button label="Reload" onClick={() => void file.refetch()} />
        </>
      }
    >
      {file.data ? <Markdown text={file.data.text} /> : null}
      {file.isPending ? <Loading /> : null}
      {file.error ? <Hint>{file.error.message}</Hint> : null}
    </FormDialog>
  );
}

/** The markdown files an answer names, to read them here. */
export function MarkdownChips({ text, scope }: { text: string; scope: FileScope | null }) {
  const paths = useMemo(() => markdownPaths(text), [text]);
  const [open, setOpen] = useState<string | null>(null);
  if (!scope || paths.length === 0) return null;
  return (
    <div {...stylex.props(styles.chips)}>
      {paths.map((p) => (
        <button key={p} type="button" onClick={() => setOpen(p)} {...stylex.props(styles.chip)}>
          <span {...stylex.props(styles.accent)}>▤</span>
          {p}
          <span {...stylex.props(styles.accent)}>View</span>
        </button>
      ))}
      {open ? <MarkdownDialog path={open} scope={scope} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}

/** Saved from the server, then offered as a download; opening is up to this computer. */
const sizeText = (size: number) =>
  size >= 1_048_576 ? `${(size / 1_048_576).toFixed(1)} MB` : `${Math.ceil(size / 1024)} KB`;

function saveBlob(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/** Files an answer kept with its message: an image shows here, anything else is saved. */
export function KeptFiles({ files }: { files: FileRef[] }) {
  const connection = useConnection();
  const [shown, setShown] = useState<{ name: string; url: string } | null>(null);
  const open = async (file: FileRef) => {
    try {
      const got = expectBody(await connection.call("file.get", { id: file.id }), "file").value;
      const bytes = Uint8Array.from(atob(got.data_base64), (c) => c.charCodeAt(0));
      const blob = new Blob([bytes], { type: got.media_type });
      if (got.media_type.startsWith("image/") && got.media_type !== "image/svg+xml") {
        setShown({ name: got.name, url: URL.createObjectURL(blob) });
      } else {
        saveBlob(blob, got.name);
      }
    } catch (e) {
      toast(e instanceof Error ? e.message : "This file did not load.");
    }
  };
  if (files.length === 0) return null;
  return (
    <div {...stylex.props(styles.chips, styles.keptChips)}>
      {files.map((f) => (
        <button key={f.id} type="button" onClick={() => void open(f)} {...stylex.props(styles.chip, styles.kept)}>
          <span {...stylex.props(styles.accent)}>{f.media_type.startsWith("image/") ? "▣" : "▤"}</span>
          {f.name}
          <span {...stylex.props(styles.size)}>{sizeText(f.size)}</span>
        </button>
      ))}
      {shown ? (
        <ImageDialog
          title={shown.name}
          width={900}
          onClose={() => {
            URL.revokeObjectURL(shown.url);
            setShown(null);
          }}
        >
          <img src={shown.url} alt={shown.name} {...stylex.props(styles.fullImage)} />
        </ImageDialog>
      ) : null}
    </div>
  );
}

const IMAGE_FILE = /\.(png|jpe?g|gif|webp|bmp)$/i;

/** The small version the server makes; the file itself only when someone opens it. */
async function fetchThumb(connection: ReturnType<typeof useConnection>, id: string): Promise<Blob> {
  const chunk = expectBody(await connection.call("upload.get", { id, max_side: 160 }), "upload").value;
  return new Blob([Uint8Array.from(atob(chunk.data_base64), (c) => c.charCodeAt(0))], { type: chunk.media_type });
}

async function fetchUpload(connection: ReturnType<typeof useConnection>, id: string): Promise<Blob> {
  const parts: Uint8Array<ArrayBuffer>[] = [];
  let type = "application/octet-stream";
  let offset = 0;
  for (;;) {
    const chunk = expectBody(await connection.call("upload.get", { id, offset }), "upload").value;
    const bytes = Uint8Array.from(atob(chunk.data_base64), (c) => c.charCodeAt(0));
    parts.push(bytes);
    type = chunk.media_type;
    if (chunk.done) break;
    offset += bytes.length;
  }
  return new Blob(parts, { type });
}

/** A small picture of an image that was sent with a message. */
function UploadThumb({ id, name, onShow }: { id: string; name: string; onShow: () => void }) {
  const connection = useConnection();
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let made: string | null = null;
    let gone = false;
    fetchThumb(connection, id)
      .then((blob) => {
        if (gone) return;
        made = URL.createObjectURL(blob);
        setUrl(made);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
      if (made) URL.revokeObjectURL(made);
    };
  }, [connection, id]);
  if (url === null) return null;
  return (
    <button type="button" aria-label={`View ${name}`} onClick={onShow} {...stylex.props(styles.thumbButton)}>
      <img src={url} alt="" {...stylex.props(styles.thumb)} />
    </button>
  );
}

export function FileChips({ files }: { files: MessageFileView[] }) {
  const connection = useConnection();
  const [shown, setShown] = useState<{ name: string; url: string } | null>(null);
  const open = async (f: MessageFileView) => {
    if (!f.id) return toast("This file is not on the server any more.");
    try {
      saveBlob(await fetchUpload(connection, f.id), f.name);
    } catch (e) {
      toast(e instanceof Error ? e.message : "This file did not load.");
    }
  };
  const show = async (f: MessageFileView) => {
    if (!f.id) return;
    try {
      setShown({ name: f.name, url: URL.createObjectURL(await fetchUpload(connection, f.id)) });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This file did not load.");
    }
  };
  if (files.length === 0) return null;
  return (
    <div {...stylex.props(styles.chips, styles.uploaded)}>
      {files.map((f) =>
        f.id && IMAGE_FILE.test(f.name) ? (
          <UploadThumb key={f.path} id={f.id} name={f.name} onShow={() => void show(f)} />
        ) : (
          <button key={f.path} type="button" onClick={() => void open(f)} {...stylex.props(styles.chip)}>
            {f.name}
          </button>
        ),
      )}
      {shown ? (
        <ImageDialog
          title={shown.name}
          width={900}
          onClose={() => {
            URL.revokeObjectURL(shown.url);
            setShown(null);
          }}
        >
          <img src={shown.url} alt={shown.name} {...stylex.props(styles.fullImage)} />
        </ImageDialog>
      ) : null}
    </div>
  );
}

/**
 * Chooses a folder where the server is: this computer's own picker when the server runs here,
 * otherwise a list the server reads.
 */
export function useFolderChooser() {
  const platform = usePlatform();
  const { remote, hostName } = useLinkState();
  const [asked, setAsked] = useState<((path: string) => void) | null>(null);
  const choose = (title: string, then: (path: string) => void) => {
    if (platform.pickFolder && !remote)
      void platform.pickFolder(title).then((path) => {
        if (path) then(path);
      });
    else setAsked(() => then);
  };
  const dialog = asked ? (
    <FolderBrowser
      title={`Choose a folder on ${hostName ?? "the server"}`}
      onClose={() => setAsked(null)}
      onPick={(path) => {
        setAsked(null);
        asked(path);
      }}
    />
  ) : null;
  return { choose, dialog };
}

/** Choosing a folder on the server, which may be another computer than this one. */
export function FolderBrowser({
  title,
  onPick,
  onClose,
}: {
  title: string;
  onPick: (path: string) => void;
  onClose: () => void;
}) {
  const connection = useConnection();
  const [path, setPath] = useState<string | null>(null);
  const [making, setMaking] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const listing = useKeyed(
    ["folders", path, version],
    async () => expectBody(await connection.call("fs.list_dir", { path }), "dir_listing").value,
  );
  const dir = listing.data;
  const entries: DirEntryView[] = dir ? (dir.path ? dir.entries : dir.roots) : [];
  const [cursor, setCursor] = useState(0);
  const at = Math.min(cursor, Math.max(entries.length - 1, 0));
  const go = (next: string | null) => {
    setPath(next);
    setCursor(0);
  };
  useKeyContext("ServerFolder", true);
  useAction("FolderNext", () => setCursor(Math.min(at + 1, entries.length - 1)));
  useAction("FolderPrev", () => setCursor(Math.max(at - 1, 0)));
  useAction("FolderOpen", () => {
    const entry = entries[at];
    if (entry) go(entry.path);
  });
  useAction("FolderParent", () => {
    if (dir?.path) go(dir.parent ?? null);
  });
  useAction("FolderUse", () => {
    if (dir?.path) onPick(dir.path);
  });
  useAction("FolderNewFolder", () => {
    if (dir?.path) setMaking("");
  });
  const make = async () => {
    if (!dir?.path || !making?.trim()) return;
    try {
      await connection.write("fs.create_dir", { name: making.trim(), parent: dir.path }, { retry: false });
      setMaking(null);
      setProblem(null);
      setVersion((v) => v + 1);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <FormDialog
      title={title}
      width={560}
      onClose={onClose}
      footer={
        <>
          {dir?.path && making === null ? (
            <span {...stylex.props(styles.footerStart)}>
              <Button label="New folder" onClick={() => setMaking("")} />
            </span>
          ) : null}
          <Button label="Cancel" onClick={onClose} />
          <Button
            variant="primary"
            label="Use this folder"
            isDisabled={!dir?.path}
            onClick={() => dir?.path && onPick(dir.path)}
          />
        </>
      }
    >
      <p {...stylex.props(styles.folderPath)}>{dir?.path ?? "Computer"}</p>
      <div {...stylex.props(styles.folders)}>
        <List density="compact">
          {dir?.path ? <ListItem label="↑ Up" onClick={() => go(dir.parent ?? null)} /> : null}
          {entries.map((e, i) => (
            <ListItem
              key={e.path}
              label={e.name}
              isSelected={i === at}
              ref={(el: HTMLElement | null) => {
                if (i === at) el?.scrollIntoView({ block: "nearest" });
              }}
              onClick={() => go(e.path)}
            />
          ))}
        </List>
        {dir && entries.length === 0 ? <Hint>No folders here.</Hint> : null}
      </div>
      {dir?.truncated ? <Hint>Only the first folders are shown.</Hint> : null}
      {making !== null ? (
        <TextInput
          hasAutoFocus
          label="New folder name"
          isLabelHidden
          placeholder="New folder name"
          value={making}
          onChange={setMaking}
          onEnter={() => void make()}
        />
      ) : null}
      {(problem ?? listing.error) ? <Problem>{problem ?? listing.error?.message}</Problem> : null}
    </FormDialog>
  );
}
