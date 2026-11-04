// What the desktop app shows around a run: its model, its cost, its trace, its helpers, and the
// files it refers to.

import { expectBody } from "@aurelius/client";
import type {
  DirEntryView,
  FileRef,
  FileScope,
  MessageFileView,
  MessageView,
  TaskRun,
  TokenUsage,
} from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { Dialog, Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { FIELD } from "./org-kit";
import { elapsedText, markdownPaths, screenshotId, traceSessionText, usageLine, waitingText } from "./run-logic";
import { modelName } from "./settings-kit";
import { costText, tokensShort } from "./usage-logic";

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
    <span title={tip} className="flex-none rounded-[5px] border border-line px-2 py-[2px] text-[12px] text-text2">
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
        <button
          type="button"
          className="h-6 cursor-pointer rounded-[5px] border border-line bg-transparent px-2 text-[12px] text-text2 hover:bg-sel"
        >
          {label}
        </button>
      }
    />
  );
}

/** The whole composed prompt of a run; the server only has it while trace mode is on. */
export function TraceLink({ run, className }: { run: string; className?: string }) {
  const connection = useConnection();
  const on = useKeyed(
    ["settings", "trace"],
    async () => expectBody(await connection.call("trace.get_setting"), "trace_setting").value,
  );
  const [open, setOpen] = useState(false);
  if (!on.data?.enabled) return null;
  return (
    <>
      <button type="button" onClick={() => setOpen(true)} className={className}>
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
    <Dialog open onClose={onClose} title="Trace" width={720}>
      <div className="flex max-h-[76vh] flex-col gap-4 overflow-y-auto p-4">
        <p className="m-0 text-[15px] font-semibold">Trace</p>
        {trace.isPending ? <p className="m-0 text-text3">Loading…</p> : null}
        {trace.error ? <p className="m-0 text-text3">{trace.error.message}</p> : null}
        {t ? (
          <>
            <p className="m-0 text-[12px] text-text2">
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
            {t.usage ? <p className="m-0 text-[12px] text-text2">{usageLine(t.usage)}</p> : null}
            <table className="w-full border-collapse text-[12px]">
              <thead className="text-left text-text3">
                <tr>
                  <th className="pb-1 font-medium">Part</th>
                  <th className="pb-1 text-right font-medium">Chars</th>
                  <th className="pb-1 text-right font-medium">≈ tokens</th>
                </tr>
              </thead>
              <tbody>
                {t.parts.map((p) => (
                  <tr key={`${p.area}|${p.label}`} className="border-t border-line2">
                    <td className="py-1">
                      {p.area} · {p.label}
                    </td>
                    <td className="py-1 text-right font-mono">{p.chars}</td>
                    <td className="py-1 text-right font-mono">{p.tokens}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {t.steps.length > 0 ? (
              <table className="w-full border-collapse text-[12px]">
                <thead className="text-left text-text3">
                  <tr>
                    <th className="pb-1 font-medium">Step</th>
                    <th className="pb-1 text-right font-medium">in</th>
                    <th className="pb-1 text-right font-medium">cache write</th>
                    <th className="pb-1 text-right font-medium">cache read</th>
                    <th className="pb-1 text-right font-medium">out</th>
                    <th className="pb-1 pl-3 font-medium">tools asked</th>
                  </tr>
                </thead>
                <tbody>
                  {t.steps.map((s, i) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: steps are fixed once read.
                    <tr key={i} className="border-t border-line2">
                      <td className="py-1">{i + 1}</td>
                      <td className="py-1 text-right font-mono">{tokensShort(s.input_tokens)}</td>
                      <td className="py-1 text-right font-mono">{tokensShort(s.cache_creation_tokens)}</td>
                      <td className="py-1 text-right font-mono">{tokensShort(s.cache_read_tokens)}</td>
                      <td className="py-1 text-right font-mono">{tokensShort(s.output_tokens)}</td>
                      <td className="py-1 pl-3">
                        {s.subagent ? "subagent · " : ""}
                        {s.tools.join(", ")}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : null}
            <TraceText title="System prompt" text={t.system_text} />
            <TraceText title="This turn" text={t.user_turn} />
            <p className="m-0 text-[12px] text-text3">Token counts per part are estimates (4 characters ≈ 1 token).</p>
          </>
        ) : null}
      </div>
      <div className="flex justify-end border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Close</Button>
      </div>
    </Dialog>
  );
}

function TraceText({ title, text }: { title: string; text: string }) {
  return (
    <details>
      <summary className="cursor-pointer text-[12px] font-medium text-text2">{title}</summary>
      <pre className="mt-2 max-h-[300px] overflow-auto rounded-[6px] bg-sunk p-2 font-mono text-[12px] whitespace-pre-wrap">
        {text}
      </pre>
    </details>
  );
}

/** The helpers of a running answer; nothing tells when they change, so it looks while the run lasts. */
export function SubagentsAtWork({ conversationId, live }: { conversationId: string; live: boolean }) {
  const connection = useConnection();
  const [agents, setAgents] = useState<
    { name: string; doing?: string | null; detail: string; since_ms: number; steps: number }[]
  >([]);
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
  if (agents.length === 0) return null;
  const now = Date.now();
  return (
    <div className="border-t border-line2 pt-4">
      <p className="mt-0 mb-2 text-[12px] font-medium text-text3">Subagents at work</p>
      {agents.map((a) => (
        <div key={`${a.name}|${a.since_ms}`} className="flex flex-col py-1 text-[12px]">
          <span className="flex justify-between gap-2">
            <span className="truncate font-medium">{a.name}</span>
            <span className="flex-none text-text3">{elapsedText(now - a.since_ms)}</span>
          </span>
          <span className="truncate text-text3">{a.doing ?? a.detail}</span>
        </div>
      ))}
    </div>
  );
}

/** Asks the employee's last question again. */
export function useRetryAnswer(conversationId: string | null | undefined, messages: MessageView[]) {
  const connection = useConnection();
  const last = [...messages].reverse().find((m) => m.role === "user");
  return () => {
    if (!last || !conversationId) return toast("Open a chat with a question first.");
    connection
      .write("chat.retry", { conversation_id: conversationId, user_message_id: last.id }, { retry: false })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
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
  return (
    <div
      role="status"
      className="flex items-center gap-[10px] rounded-[8px] bg-amber-soft px-3 py-2 text-[12px] shadow-[inset_0_0_0_1px_var(--amber-line)]"
    >
      <span className="size-[7px] flex-none rounded-full bg-amber" />
      {text}
    </div>
  );
}

function ScreenshotDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const connection = useConnection();
  const shot = useKeyed(
    ["screenshot", id],
    async () => expectBody(await connection.call("screenshot.get", { id }), "screenshot").value,
  );
  return (
    <Dialog open onClose={onClose} title="Screenshot" width={960}>
      <div className="flex max-h-[80vh] justify-center overflow-auto p-4">
        {shot.data ? (
          <img
            alt="What the employee saw"
            className="max-w-full rounded-[6px]"
            src={`data:image/jpeg;base64,${shot.data.jpeg_base64}`}
          />
        ) : (
          <p className="m-0 text-text3">{shot.error ? "This screenshot is no longer there." : "Loading…"}</p>
        )}
      </div>
    </Dialog>
  );
}

/** A screenshot step's link; other steps have none. */
export function ScreenshotLink({
  tool,
  output,
  className,
}: {
  tool: string;
  output?: string | null;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const id = tool === "mcp__aurelius__screenshot" ? screenshotId(output ?? "") : null;
  if (!id) return null;
  return (
    <>
      <button type="button" className={className} onClick={() => setOpen(true)}>
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
    <Dialog open onClose={onClose} title={path} width={760}>
      <div className="flex max-h-[76vh] flex-col gap-2 overflow-y-auto p-4">
        <p className="m-0 font-mono text-[12px] text-text3">{path}</p>
        {file.data ? <Markdown text={file.data.text} /> : null}
        {file.isPending ? <p className="m-0 text-text3">Loading…</p> : null}
        {file.error ? <p className="m-0 text-text3">{file.error.message}</p> : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        {file.data ? (
          <Button
            onClick={() => {
              void platform.copy(file.data.text).then(() => toast("Copied the Markdown"));
            }}
          >
            Copy raw
          </Button>
        ) : null}
        <Button onClick={() => void file.refetch()}>Reload</Button>
      </div>
    </Dialog>
  );
}

/** The markdown files an answer names, to read them here. */
export function MarkdownChips({ text, scope }: { text: string; scope: FileScope | null }) {
  const paths = useMemo(() => markdownPaths(text), [text]);
  const [open, setOpen] = useState<string | null>(null);
  if (!scope || paths.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-[6px] pt-[2px]">
      {paths.map((p) => (
        <button
          key={p}
          type="button"
          onClick={() => setOpen(p)}
          className="flex cursor-pointer items-center gap-[6px] rounded-[6px] border border-line bg-transparent px-[9px] py-1 text-[12px] text-text hover:border-btn-line"
        >
          <span className="text-acc-text">▤</span>
          {p}
          <span className="text-acc-text">View</span>
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
    <div className="flex flex-wrap gap-2 pt-[2px]">
      {files.map((f) => (
        <button
          key={f.id}
          type="button"
          onClick={() => void open(f)}
          className="flex cursor-pointer items-center gap-2 rounded-[7px] border border-line bg-transparent px-[10px] py-[6px] text-[12.5px] text-text hover:border-btn-line"
        >
          <span className="text-acc-text">{f.media_type.startsWith("image/") ? "▣" : "▤"}</span>
          {f.name}
          <span className="text-[11px] text-text3">{sizeText(f.size)}</span>
        </button>
      ))}
      {shown ? (
        <Dialog
          open
          onClose={() => {
            URL.revokeObjectURL(shown.url);
            setShown(null);
          }}
          title={shown.name}
          width={900}
        >
          <img src={shown.url} alt={shown.name} className="block max-h-[80vh] w-full object-contain" />
        </Dialog>
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
    <button
      type="button"
      aria-label={`View ${name}`}
      onClick={onShow}
      className="cursor-pointer border-0 bg-transparent p-0"
    >
      <img src={url} alt="" className="block h-[56px] max-w-[96px] rounded-[6px] object-cover" />
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
    <div className="flex flex-wrap items-center gap-[6px]">
      {files.map((f) =>
        f.id && IMAGE_FILE.test(f.name) ? (
          <UploadThumb key={f.path} id={f.id} name={f.name} onShow={() => void show(f)} />
        ) : (
          <button
            key={f.path}
            type="button"
            onClick={() => void open(f)}
            className="cursor-pointer rounded-[6px] border border-line bg-transparent px-[9px] py-1 text-[12px] text-text hover:border-btn-line"
          >
            {f.name}
          </button>
        ),
      )}
      {shown ? (
        <Dialog
          open
          onClose={() => {
            URL.revokeObjectURL(shown.url);
            setShown(null);
          }}
          title={shown.name}
          width={900}
        >
          <img src={shown.url} alt={shown.name} className="block max-h-[80vh] w-full object-contain" />
        </Dialog>
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
    <Dialog open onClose={onClose} title={title} width={560}>
      <div className="flex flex-col gap-2 p-4">
        <p className="m-0 text-[15px] font-semibold">{title}</p>
        <p className="m-0 truncate font-mono text-[12px] text-text3">{dir?.path ?? "Computer"}</p>
        <div className="h-[280px] overflow-y-auto rounded-[6px] border border-line">
          {dir?.path ? (
            <button
              type="button"
              onClick={() => go(dir.parent ?? null)}
              className="block w-full cursor-pointer border-0 bg-transparent px-3 py-[6px] text-left text-text2 hover:bg-sel"
            >
              ↑ Up
            </button>
          ) : null}
          {entries.map((e, i) => (
            <button
              key={e.path}
              type="button"
              aria-current={i === at ? "true" : undefined}
              ref={(el) => {
                if (i === at) el?.scrollIntoView({ block: "nearest" });
              }}
              onClick={() => go(e.path)}
              className="block w-full cursor-pointer truncate border-0 bg-transparent aria-[current]:bg-sel px-3 py-[6px] text-left text-text hover:bg-sel"
            >
              {e.name}
            </button>
          ))}
          {dir && entries.length === 0 ? <p className="m-0 px-3 py-2 text-text3">No folders here.</p> : null}
        </div>
        {dir?.truncated ? <p className="m-0 text-[12px] text-text3">Only the first folders are shown.</p> : null}
        {making !== null ? (
          <input
            autoFocus
            aria-label="New folder name"
            placeholder="New folder name"
            className={FIELD}
            value={making}
            onChange={(e) => setMaking(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void make();
              }
            }}
          />
        ) : null}
        {(problem ?? listing.error) ? (
          <p role="alert" className="m-0 text-red">
            {problem ?? listing.error?.message}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        {dir?.path && making === null ? (
          <Button className="mr-auto" onClick={() => setMaking("")}>
            New folder
          </Button>
        ) : null}
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={!dir?.path} onClick={() => dir?.path && onPick(dir.path)}>
          Use this folder
        </Button>
      </div>
    </Dialog>
  );
}
