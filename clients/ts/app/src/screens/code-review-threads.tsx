// The review of one branch on the server (ticket #84, feature `code_review`): its diff with a "+" on every line
// for a comment, the line threads with their replies and Resolve, and the files the board member viewed. A new
// comment or reply is pending until Finish review, so the owner is woken once with all of it.

import { Button } from "@astryxdesign/core/Button";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { TextArea } from "@astryxdesign/core/TextArea";
import { expectBody } from "@aurelius/client";
import type { CodeDiff, CodeDiffFile, CodeReviewView, CodeThreadView, MethodParams } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, type ReactNode, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { Icon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { ago } from "./code-logic";
import {
  type DiffRow,
  diffRows,
  fileLabel,
  openThreadsIn,
  outdated,
  STATUS_LETTER,
  snippetAt,
  threadsOn,
} from "./code-review-logic";

/** The server feature of the diff, the line threads and the viewed files. */
export const CODE_REVIEW = "code_review";

const MONO = "var(--font-family-code)";

const styles = stylex.create({
  files: {
    display: "grid",
    gridTemplateColumns: { default: "250px minmax(0, 1fr)", "@media (max-width: 1180px)": "minmax(0, 1fr)" },
    gap: 14,
    alignItems: "start",
  },
  tree: {
    position: { default: "sticky", "@media (max-width: 1180px)": "static" },
    top: 0,
    display: "flex",
    flexDirection: "column",
    gap: 1,
    padding: 6,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
  },
  prog: {
    display: "flex",
    flexDirection: "column",
    gap: 5,
    paddingTop: 4,
    paddingBottom: 8,
    paddingInline: 6,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  bar: { height: 4, borderRadius: 2, backgroundColor: "var(--color-border-emphasized)", overflow: "hidden" },
  barFill: { display: "block", height: "100%", backgroundColor: "var(--color-text-success)" },
  tf: {
    display: "grid",
    gridTemplateColumns: "14px minmax(0, 1fr) auto",
    alignItems: "center",
    gap: 6,
    paddingBlock: 4,
    paddingInline: 6,
    borderWidth: 0,
    borderRadius: 6,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  fn: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontFamily: MONO,
    fontSize: 11.5,
  },
  fnViewed: { color: "var(--precision-text3)", textDecorationLine: "line-through" },
  st: { fontFamily: MONO, fontSize: 10.5, fontWeight: 700 },
  st_A: { color: "var(--color-text-success)" },
  st_M: { color: "var(--color-text-warning)" },
  st_D: { color: "var(--color-text-error)" },
  st_R: { color: "var(--color-text-info)" },
  cm: {
    display: "inline-flex",
    alignItems: "center",
    gap: 3,
    fontSize: 10.5,
    fontWeight: 700,
    color: "var(--color-text-info)",
  },
  unc: { fontSize: 10.5, fontWeight: 700, color: "var(--color-text-warning)", whiteSpace: "nowrap" },
  size: { fontFamily: MONO, fontSize: 11.5, whiteSpace: "nowrap" },
  add: { color: "var(--color-text-success)" },
  del: { color: "var(--color-text-error)" },
  diffs: { display: "flex", flexDirection: "column", gap: 12, minWidth: 0 },
  df: {
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
    overflow: "hidden",
  },
  dfHead: {
    position: "sticky",
    top: 0,
    zIndex: 1,
    display: "flex",
    alignItems: "center",
    gap: 8,
    paddingBlock: 6,
    paddingInline: 10,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: "var(--color-background-surface)",
  },
  path: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontFamily: MONO,
    fontSize: 12,
  },
  grow: { flexGrow: 1 },
  fold: {
    width: 20,
    borderWidth: 0,
    backgroundColor: "transparent",
    color: "var(--precision-text3)",
    cursor: "pointer",
    font: "inherit",
  },
  dfBody: { overflowX: "auto" },
  note: { paddingBlock: 10, paddingInline: 12, fontSize: 12.5, color: "var(--precision-text3)" },
  table: { borderCollapse: "collapse", width: "100%", fontFamily: MONO, fontSize: 12, lineHeight: 1.55 },
  hunk: {
    paddingBlock: 2,
    paddingInline: 8,
    backgroundColor: "var(--color-info-muted)",
    color: "var(--color-text-info)",
    whiteSpace: "pre",
  },
  ln: {
    position: "relative",
    width: "1%",
    paddingInline: 6,
    textAlign: "end",
    color: "var(--precision-text3)",
    userSelect: "none",
    verticalAlign: "top",
    whiteSpace: "pre",
  },
  lnNew: { paddingInlineEnd: 22 },
  code: { paddingInline: 8, whiteSpace: "pre", verticalAlign: "top" },
  pm: { color: "var(--precision-text3)", userSelect: "none" },
  row_add: { backgroundColor: "var(--color-success-muted)" },
  row_del: { backgroundColor: "var(--color-error-muted)" },
  row_ctx: {},
  addc: {
    position: "absolute",
    insetInlineEnd: 2,
    top: 1,
    width: 17,
    height: 17,
    padding: 0,
    borderWidth: 0,
    borderRadius: 4,
    backgroundColor: "var(--color-accent)",
    color: "var(--color-on-accent)",
    fontSize: 13,
    lineHeight: "17px",
    cursor: "pointer",
    opacity: { default: 0, ":hover": 1, ":focus-visible": 1 },
  },
  cmtRow: {
    paddingBlock: 8,
    paddingInline: 10,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: "var(--color-background-surface)",
    fontFamily: "var(--font-family-body, inherit)",
    fontSize: 13,
    whiteSpace: "normal",
  },
  // A thread
  thread: {
    maxWidth: 760,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
    overflow: "hidden",
    fontFamily: "var(--font-family-body, inherit)",
  },
  thHead: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    minWidth: 0,
    paddingBlock: 7,
    paddingInline: 12,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: "var(--color-background-surface)",
    fontSize: 12,
  },
  faint: { color: "var(--precision-text3)", whiteSpace: "nowrap" },
  snip: { borderBottomWidth: 1, borderBottomStyle: "solid", borderBottomColor: "var(--precision-line2)" },
  c1: {
    display: "flex",
    gap: 9,
    paddingBlock: 10,
    paddingInline: 12,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  c1First: { borderTopWidth: 0 },
  c1b: { display: "flex", flexDirection: "column", gap: 2, minWidth: 0, flexGrow: 1 },
  c1h: { display: "flex", alignItems: "center", gap: 6, fontSize: 12.5 },
  who: { fontWeight: 600, color: "var(--color-text-primary)" },
  txt: { whiteSpace: "pre-wrap", overflowWrap: "anywhere" },
  thFoot: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    flexWrap: "wrap",
    paddingBlock: 8,
    paddingInline: 12,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    backgroundColor: "var(--color-background-surface)",
  },
  reply: {
    flexGrow: 1,
    minWidth: 150,
    paddingBlock: 5,
    paddingInline: 9,
    borderRadius: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-background-card)",
    color: "var(--precision-text3)",
    font: "inherit",
    textAlign: "start",
    cursor: "text",
  },
  box: { display: "flex", flexDirection: "column", gap: 6, maxWidth: 640, width: "100%" },
  acts: { display: "flex", justifyContent: "flex-end", gap: 6 },
  thFold: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    width: "100%",
    paddingBlock: 8,
    paddingInline: 12,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: "var(--color-text-secondary)",
    font: "inherit",
    fontSize: 12.5,
    textAlign: "start",
    cursor: "pointer",
  },
  chip: {
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    paddingInline: 7,
    paddingBlock: 1,
    borderRadius: 999,
    fontSize: 11,
    fontWeight: 600,
    whiteSpace: "nowrap",
  },
  chip_ok: { backgroundColor: "var(--color-success-muted)", color: "var(--color-text-success)" },
  chip_warn: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
  chip_n: { backgroundColor: "var(--color-background-muted)", color: "var(--color-text-secondary)" },
});

type Status = keyof typeof STATUS_STYLE;
const STATUS_STYLE = { A: styles.st_A, M: styles.st_M, D: styles.st_D, R: styles.st_R } as const;

function Chip({ tone, children }: { tone: "ok" | "warn" | "n"; children: ReactNode }) {
  return <span {...stylex.props(styles.chip, styles[`chip_${tone}`])}>{children}</span>;
}

/** What merging the branch brings into main, read again when its head changes. */
export function useCodeDiff(projectId: string, branch: string, head: string, wanted: boolean) {
  const connection = useConnection();
  const { status, user, features } = useLinkState();
  return useQuery({
    queryKey: ["code", "diff", projectId, branch, head],
    enabled: wanted && features.includes(CODE_REVIEW) && status === "online" && user !== null,
    staleTime: 60_000,
    queryFn: async () =>
      expectBody(await connection.call("code.diff", { project_id: projectId, branch }), "code_diff").value,
  });
}

/** The branch's review as the board member sees it; `code.review_changed` reads it again. */
export function useCodeReviewRead(projectId: string, branch: string, head: string, wanted = true) {
  const connection = useConnection();
  const { features } = useLinkState();
  return useKeyed<CodeReviewView>(
    ["code_review", projectId, branch, head],
    async () =>
      expectBody(await connection.call("code.review", { project_id: projectId, branch, head }), "code_review").value,
    wanted && features.includes(CODE_REVIEW),
  );
}

type ReviewWrite = "code.start_thread" | "code.reply" | "code.resolve_thread" | "code.set_viewed";

/** One review write at a time; the review is read again after it. A failure is a toast, the draft stays. */
export function useReviewWrite() {
  const connection = useConnection();
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const run = async <M extends ReviewWrite>(method: M, params: MethodParams[M]): Promise<boolean> => {
    if (busy) return false;
    setBusy(true);
    try {
      await connection.write(method, params, { retry: false });
      await client.invalidateQueries({ queryKey: ["code_review"] });
      return true;
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
      return false;
    } finally {
      setBusy(false);
    }
  };
  return { busy, run };
}

/** A small comment box: a reply in a thread or the first comment on a line. */
function CommentBox({
  placeholder,
  busy,
  onAdd,
  onCancel,
}: {
  placeholder: string;
  busy: boolean;
  onAdd: (text: string) => Promise<boolean>;
  onCancel: () => void;
}) {
  const [text, setText] = useState("");
  const add = async () => {
    if (!text.trim() || busy) return;
    if (await onAdd(text.trim())) setText("");
  };
  return (
    <div {...stylex.props(styles.box)}>
      <TextArea
        hasAutoFocus
        label="Comment"
        isLabelHidden
        placeholder={placeholder}
        rows={3}
        value={text}
        onChange={setText}
      />
      <div {...stylex.props(styles.acts)}>
        <Button size="sm" variant="ghost" label="Cancel" onClick={onCancel} />
        <Button
          size="sm"
          variant="primary"
          label="Add to review"
          isDisabled={busy || !text.trim()}
          onClick={() => void add()}
        />
      </div>
    </div>
  );
}

function SnippetTable({ lines }: { lines: string[] }) {
  return (
    <table {...stylex.props(styles.table, styles.snip)}>
      <tbody>
        {lines.map((raw, i) => {
          const kind = raw.startsWith("+") ? "add" : raw.startsWith("-") ? "del" : "ctx";
          return (
            // biome-ignore lint/suspicious/noArrayIndexKey: a snippet's lines are fixed and may repeat
            <tr key={i} {...stylex.props(styles[`row_${kind}`])}>
              <td {...stylex.props(styles.code)}>
                <span {...stylex.props(styles.pm)}>{raw.charAt(0) || " "}</span>
                {raw.slice(1)}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/**
 * One line thread: its comments (yours pending until Finish review), a reply box and Resolve. A resolved thread
 * folds until opened. In the Conversation it also shows its file, line and the code it is on.
 */
export function ThreadCard({
  thread,
  head,
  inConversation,
  owner,
  now,
}: {
  thread: CodeThreadView;
  head: string;
  inConversation: boolean;
  owner: string;
  now: number;
}) {
  const write = useReviewWrite();
  const [open, setOpen] = useState(false);
  const [replying, setReplying] = useState(false);
  const comments = thread.comments ?? [];
  const head_ = inConversation ? (
    <div {...stylex.props(styles.thHead)}>
      <span title={thread.path} {...stylex.props(styles.path)}>
        {thread.path}
      </span>
      <span {...stylex.props(styles.faint)}>line {thread.line}</span>
      {outdated(thread, head) ? <Chip tone="n">Outdated</Chip> : null}
      <span {...stylex.props(styles.grow)} />
      {thread.resolved ? <Chip tone="ok">✓ Resolved</Chip> : null}
    </div>
  ) : null;
  if (thread.resolved && !open) {
    return (
      <div {...stylex.props(styles.thread)}>
        {head_}
        <button type="button" onClick={() => setOpen(true)} {...stylex.props(styles.thFold)}>
          {inConversation ? null : <Chip tone="ok">✓ Resolved</Chip>}
          {comments.length} {comments.length === 1 ? "comment" : "comments"} · Show
        </button>
      </div>
    );
  }
  return (
    <article aria-label={`Comments on ${thread.path} line ${thread.line}`} {...stylex.props(styles.thread)}>
      {head_}
      {inConversation && (thread.snippet ?? []).length > 0 ? <SnippetTable lines={thread.snippet ?? []} /> : null}
      <div>
        {comments.map((c, i) => (
          <div key={c.id} {...stylex.props(styles.c1, i === 0 && styles.c1First)}>
            <Mark letter={initial(c.username)} color={markColor(c.username)} size={20} radius={10} />
            <div {...stylex.props(styles.c1b)}>
              <div {...stylex.props(styles.c1h)}>
                <span {...stylex.props(styles.who)}>{c.mine ? "You" : c.username}</span>
                {c.pending ? (
                  <Chip tone="warn">Pending</Chip>
                ) : (
                  <span {...stylex.props(styles.faint)}>{ago(c.created_at_ms, now)}</span>
                )}
              </div>
              <div {...stylex.props(styles.txt)}>{c.body}</div>
            </div>
          </div>
        ))}
      </div>
      <div {...stylex.props(styles.thFoot)}>
        {replying ? (
          <CommentBox
            placeholder={`Reply to ${owner}…`}
            busy={write.busy}
            onCancel={() => setReplying(false)}
            onAdd={async (body) => {
              const ok = await write.run("code.reply", { thread_id: thread.id, body });
              if (ok) setReplying(false);
              return ok;
            }}
          />
        ) : (
          <>
            <button type="button" onClick={() => setReplying(true)} {...stylex.props(styles.reply)}>
              Reply…
            </button>
            <Button
              size="sm"
              label={thread.resolved ? "Unresolve" : "Resolve conversation"}
              isDisabled={write.busy}
              onClick={() => {
                setOpen(false);
                void write.run("code.resolve_thread", { thread_id: thread.id, resolved: !thread.resolved });
              }}
            />
          </>
        )}
      </div>
    </article>
  );
}

function FileDiff({
  file,
  projectId,
  branch,
  head,
  threads,
  viewed,
  owner,
  now,
  write,
}: {
  file: CodeDiffFile;
  projectId: string;
  branch: string;
  head: string;
  threads: CodeThreadView[];
  viewed: boolean;
  owner: string;
  now: number;
  write: ReturnType<typeof useReviewWrite>;
}) {
  const [folded, setFolded] = useState<boolean | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  // Viewed flips at once; the server's answer (or a failure) replaces it.
  const [ticked, setTicked] = useState<boolean | null>(null);
  const isViewed = ticked ?? viewed;
  // Viewed folds a file until it is opened again.
  const isFolded = folded ?? isViewed;
  const letter = STATUS_LETTER[file.status];
  const hunks = file.hunks ?? [];
  let body: ReactNode;
  if (file.binary) body = <div {...stylex.props(styles.note)}>A binary file; git shows no lines.</div>;
  else if (hunks.length === 0)
    body = (
      <div {...stylex.props(styles.note)}>
        {file.status === "renamed" ? `Renamed from ${file.old_path}.` : "No lines changed."}
      </div>
    );
  else
    body = (
      <table {...stylex.props(styles.table)}>
        <tbody>
          {hunks.map((hunk) => {
            const rows = diffRows(hunk);
            return (
              <Fragment key={`${hunk.old_start}:${hunk.new_start}:${hunk.header}`}>
                <tr>
                  <td colSpan={3} {...stylex.props(styles.hunk)}>
                    {hunk.header}
                  </td>
                </tr>
                {rows.map((row: DiffRow, i) => {
                  // A line's numbers on both sides name it within its file.
                  const key = `${row.old ?? ""}:${row.new ?? ""}`;
                  const here = file.uncommitted ? [] : threadsOn(threads, file.path, row.side, row.line);
                  return (
                    <Fragment key={key}>
                      <tr {...stylex.props(styles[`row_${row.kind}`])}>
                        <td {...stylex.props(styles.ln)}>{row.old ?? ""}</td>
                        <td {...stylex.props(styles.ln, styles.lnNew)}>
                          {row.new ?? ""}
                          {file.uncommitted ? null : (
                            <button
                              type="button"
                              title="Comment on this line"
                              aria-label={`Comment on ${file.path} line ${row.line}`}
                              onClick={() => setDraft(key)}
                              {...stylex.props(styles.addc)}
                            >
                              +
                            </button>
                          )}
                        </td>
                        <td {...stylex.props(styles.code)}>
                          <span {...stylex.props(styles.pm)}>{row.raw.charAt(0) || " "}</span>
                          {row.raw.slice(1)}
                        </td>
                      </tr>
                      {here.map((t) => (
                        <tr key={t.id}>
                          <td colSpan={3} {...stylex.props(styles.cmtRow)}>
                            <ThreadCard thread={t} head={head} inConversation={false} owner={owner} now={now} />
                          </td>
                        </tr>
                      ))}
                      {draft === key ? (
                        <tr>
                          <td colSpan={3} {...stylex.props(styles.cmtRow)}>
                            <CommentBox
                              placeholder={`Comment on line ${row.line}…`}
                              busy={write.busy}
                              onCancel={() => setDraft(null)}
                              onAdd={async (text) => {
                                const ok = await write.run("code.start_thread", {
                                  project_id: projectId,
                                  branch,
                                  path: file.path,
                                  line: row.line,
                                  side: row.side,
                                  commit: head,
                                  snippet: snippetAt(rows, i),
                                  body: text,
                                });
                                if (ok) setDraft(null);
                                return ok;
                              }}
                            />
                          </td>
                        </tr>
                      ) : null}
                    </Fragment>
                  );
                })}
              </Fragment>
            );
          })}
        </tbody>
      </table>
    );
  return (
    <section aria-label={file.path} id={fileAnchor(file)} {...stylex.props(styles.df)}>
      <div {...stylex.props(styles.dfHead)}>
        <button
          type="button"
          aria-label={isFolded ? `Show ${file.path}` : `Fold ${file.path}`}
          aria-expanded={!isFolded}
          onClick={() => setFolded(!isFolded)}
          {...stylex.props(styles.fold)}
        >
          {isFolded ? "▸" : "▾"}
        </button>
        <span {...stylex.props(styles.st, STATUS_STYLE[letter as Status])}>{letter}</span>
        <span title={file.path} {...stylex.props(styles.path)}>
          {file.path}
        </span>
        {file.uncommitted ? <Chip tone="warn">Not committed</Chip> : null}
        {file.truncated ? <Chip tone="n">Cut short</Chip> : null}
        <span {...stylex.props(styles.grow)} />
        <span {...stylex.props(styles.size)}>
          <span {...stylex.props(styles.add)}>+{file.additions}</span>{" "}
          <span {...stylex.props(styles.del)}>−{file.deletions}</span>
        </span>
        {file.uncommitted ? null : (
          <CheckboxInput
            label="Viewed"
            value={isViewed}
            isDisabled={write.busy}
            onChange={() => {
              setFolded(null);
              setTicked(!isViewed);
              void write
                .run("code.set_viewed", {
                  project_id: projectId,
                  branch,
                  head,
                  path: file.path,
                  viewed: !isViewed,
                })
                .finally(() => setTicked(null));
            }}
          />
        )}
      </div>
      {isFolded ? null : <div {...stylex.props(styles.dfBody)}>{body}</div>}
    </section>
  );
}

function fileAnchor(file: CodeDiffFile): string {
  return `diff-${file.uncommitted ? "u-" : ""}${file.path.replace(/[^A-Za-z0-9_-]/g, "-")}`;
}

/** Files changed: a tree with how many files were viewed, and every file's diff with its threads. */
export function FilesChanged({
  diff,
  review,
  projectId,
  owner,
  now,
}: {
  diff: CodeDiff;
  review: CodeReviewView | undefined;
  projectId: string;
  owner: string;
  now: number;
}) {
  const write = useReviewWrite();
  const files = diff.files ?? [];
  const threads = review?.threads ?? [];
  const viewed = new Set(review?.viewed ?? []);
  const reviewable = files.filter((f) => !f.uncommitted);
  const seen = reviewable.filter((f) => viewed.has(f.path)).length;
  if (files.length === 0) return <EmptyState isCompact title="This branch changes no files against main." />;
  return (
    <div {...stylex.props(styles.files)}>
      <nav aria-label="Changed files" {...stylex.props(styles.tree)}>
        <div {...stylex.props(styles.prog)}>
          <span>
            {seen} of {reviewable.length} files viewed
          </span>
          <span {...stylex.props(styles.bar)}>
            <span
              {...stylex.props(styles.barFill)}
              style={{ width: `${reviewable.length ? (seen / reviewable.length) * 100 : 0}%` }}
            />
          </span>
        </div>
        {files.map((f) => {
          const letter = STATUS_LETTER[f.status];
          const open = f.uncommitted ? 0 : openThreadsIn(threads, f.path);
          return (
            <button
              key={fileAnchor(f)}
              type="button"
              title={`${f.path}${f.uncommitted ? " · not committed" : ""}`}
              onClick={() =>
                document.getElementById(fileAnchor(f))?.scrollIntoView({ behavior: "smooth", block: "start" })
              }
              {...stylex.props(styles.tf)}
            >
              <span {...stylex.props(styles.st, STATUS_STYLE[letter as Status])}>{letter}</span>
              <span {...stylex.props(styles.fn, !f.uncommitted && viewed.has(f.path) && styles.fnViewed)}>
                {fileLabel(f.path)}
              </span>
              {open ? (
                <span
                  title={`${open} open ${open === 1 ? "conversation" : "conversations"}`}
                  {...stylex.props(styles.cm)}
                >
                  <Icon name="chat" size={11} /> {open}
                </span>
              ) : f.uncommitted ? (
                <span {...stylex.props(styles.unc)}>not committed</span>
              ) : (
                <span {...stylex.props(styles.size)}>
                  <span {...stylex.props(styles.add)}>+{f.additions}</span>{" "}
                  <span {...stylex.props(styles.del)}>−{f.deletions}</span>
                </span>
              )}
            </button>
          );
        })}
        {diff.truncated ? <div {...stylex.props(styles.note)}>The diff is long; some of it is left out.</div> : null}
      </nav>
      <div {...stylex.props(styles.diffs)}>
        {files.map((f) => (
          <FileDiff
            key={fileAnchor(f)}
            file={f}
            projectId={projectId}
            branch={diff.branch}
            head={diff.head}
            threads={threads}
            viewed={!f.uncommitted && viewed.has(f.path)}
            owner={owner}
            now={now}
            write={write}
          />
        ))}
      </div>
    </div>
  );
}
