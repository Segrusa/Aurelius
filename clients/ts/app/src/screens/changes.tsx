// Changes: what a project's work folder looks like in git, to check what employees did without a
// terminal on the server. What is not committed yet, the commits of a branch and the branches,
// each with the text of the change. Read-only: nothing here changes the repository.

import { Button } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { expectBody } from "@aurelius/client";
import type { GitBranch, GitChange, GitCommit } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useMemo } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed, useProjects } from "../data";
import { usePlatform } from "../platform";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import {
  ago,
  type ChangesSearch,
  type ChangesView,
  changesLine,
  counts,
  type DiffFile,
  GIT_FEATURE,
  parseDiff,
  shortId,
  splitPath,
  statusOf,
  statusTone,
  trackLine,
} from "./changes-logic";
import { Empty, failureOf, Hint, Loading, Page, PageHead, Problem } from "./org-kit";

const HEAD = "__head__";

const styles = stylex.create({
  body: { display: "flex", minHeight: 0, flex: 1 },
  list: {
    display: "flex",
    width: 360,
    flexShrink: 0,
    flexDirection: "column",
    borderRightWidth: 1,
    borderRightStyle: "solid",
    borderRightColor: "var(--color-border)",
    minHeight: 0,
  },
  listHead: { display: "flex", flexDirection: "column", gap: 8, padding: 12 },
  listScroll: { minHeight: 0, flex: 1, overflowY: "auto" },
  detail: { minWidth: 0, flex: 1, overflowY: "auto" },
  detailPad: { display: "flex", flexDirection: "column", gap: 16, padding: 16 },
  row: {
    display: "flex",
    width: "100%",
    alignItems: "baseline",
    gap: 8,
    paddingInline: 12,
    paddingBlock: 6,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    textAlign: "left",
    cursor: "pointer",
    color: "var(--color-text-primary)",
    fontSize: 13,
  },
  rowChosen: { backgroundColor: "var(--color-overlay-hover)" },
  column: { display: "flex", minWidth: 0, flex: 1, flexDirection: "column", gap: 2 },
  truncate: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  quiet: { fontSize: 12, color: "var(--precision-text3)" },
  secondary: { color: "var(--color-text-secondary)" },
  letter: { width: 14, flexShrink: 0, fontFamily: "var(--font-family-code)", fontSize: 12, fontWeight: 600 },
  add: { color: "var(--color-text-success)" },
  del: { color: "var(--color-text-error)" },
  warn: { color: "var(--color-text-warning)" },
  count: { flexShrink: 0, fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--precision-text3)" },
  chips: { display: "flex", flexWrap: "wrap", gap: 4 },
  chip: {
    paddingInline: 6,
    borderRadius: 4,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    fontSize: 11,
    color: "var(--color-text-secondary)",
  },
  more: { display: "flex", justifyContent: "center", padding: 12 },
  title: { fontSize: 16, fontWeight: 600, color: "var(--color-text-primary)", overflowWrap: "anywhere" },
  message: {
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
    fontSize: 13,
    color: "var(--color-text-secondary)",
    margin: 0,
    fontFamily: "var(--font-family-body)",
  },
  meta: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  code: { fontFamily: "var(--font-family-code)" },
  file: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 6,
    overflow: "hidden",
  },
  fileHead: {
    display: "flex",
    alignItems: "baseline",
    gap: 8,
    paddingInline: 12,
    paddingBlock: 8,
    backgroundColor: "var(--precision-sunk)",
    fontSize: 13,
    fontFamily: "var(--font-family-code)",
  },
  lines: { overflowX: "auto", fontFamily: "var(--font-family-code)", fontSize: 12, lineHeight: 1.5 },
  line: { display: "flex", minWidth: "max-content" },
  number: {
    width: 44,
    flexShrink: 0,
    paddingInline: 6,
    textAlign: "right",
    color: "var(--precision-text3)",
    userSelect: "none",
  },
  sign: { width: 14, flexShrink: 0, textAlign: "center", userSelect: "none" },
  text: { whiteSpace: "pre", paddingRight: 12 },
  lineAdd: { backgroundColor: "var(--color-success-muted)" },
  lineDel: { backgroundColor: "var(--color-error-muted)" },
  lineHunk: { backgroundColor: "var(--precision-sunk)", color: "var(--precision-text3)" },
  lineNote: { color: "var(--precision-text3)", fontStyle: "italic" },
  branches: { display: "flex", flexDirection: "column", padding: 12, maxWidth: 960 },
  branch: {
    display: "flex",
    alignItems: "baseline",
    gap: 12,
    paddingInline: 12,
    paddingBlock: 8,
    borderWidth: 0,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    textAlign: "left",
    cursor: "pointer",
    color: "var(--color-text-primary)",
    fontSize: 13,
  },
  branchName: { width: 260, flexShrink: 0, fontFamily: "var(--font-family-code)", overflowWrap: "anywhere" },
  center: { display: "flex", flex: 1, alignItems: "center", justifyContent: "center", padding: 24 },
  pick: { display: "flex", flexDirection: "column", gap: 4, padding: 24, maxWidth: 480 },
});

const TONES = { add: styles.add, del: styles.del, warn: styles.warn, plain: null } as const;

function useGit(project: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["git", "overview", project],
    async () =>
      expectBody(await connection.call("git.overview", { project_id: project as string }), "git_overview").value,
    project !== null,
  );
}

/** Everything a branch changed since it left its base, in place of one commit. */
type Range = { rev: string; base: string };

function useDiff(project: string, commit: string | null, path: string | null, range: Range | null) {
  const connection = useConnection();
  return useKeyed(
    ["git", "diff", project, commit, path, range?.rev ?? null, range?.base ?? null],
    async () =>
      expectBody(
        await connection.call("git.diff", {
          project_id: project,
          ...(commit ? { id: commit } : {}),
          ...(range ? { rev: range.rev, base: range.base } : {}),
          ...(path ? { path } : {}),
        }),
        "git_diff",
      ).value,
  );
}

function ChangeRow({ change, chosen, onClick }: { change: GitChange; chosen: boolean; onClick: () => void }) {
  const status = statusOf(change.status);
  const { dir, name } = splitPath(change.path);
  return (
    <button type="button" onClick={onClick} {...stylex.props(styles.row, chosen && styles.rowChosen)}>
      <span title={status.label} {...stylex.props(styles.letter, TONES[statusTone(change.status)])}>
        {status.letter}
      </span>
      <span {...stylex.props(styles.column)}>
        <span title={change.path} {...stylex.props(styles.truncate)}>
          {name}
          {change.staged ? <span {...stylex.props(styles.quiet)}> · staged</span> : null}
        </span>
        {dir || change.from ? (
          <span {...stylex.props(styles.quiet, styles.truncate)}>
            {change.from ? `${change.from} → ${dir || "./"}` : dir}
          </span>
        ) : null}
      </span>
      <span {...stylex.props(styles.count)}>{counts(change)}</span>
    </button>
  );
}

function FileDiff({ file }: { file: DiffFile }) {
  const status = statusOf(file.status);
  return (
    <section {...stylex.props(styles.file)}>
      <div {...stylex.props(styles.fileHead)}>
        <span title={status.label} {...stylex.props(styles.letter, TONES[statusTone(file.status)])}>
          {status.letter}
        </span>
        <span {...stylex.props(styles.column)}>{file.from ? `${file.from} → ${file.path}` : file.path}</span>
        <span {...stylex.props(styles.count)}>{counts(file)}</span>
      </div>
      {file.binary ? (
        <div {...stylex.props(styles.detailPad, styles.quiet)}>A binary file: there is no text to show.</div>
      ) : file.lines.length === 0 ? (
        <div {...stylex.props(styles.detailPad, styles.quiet)}>No lines changed.</div>
      ) : (
        <div {...stylex.props(styles.lines)}>
          {file.lines.map((line, i) => (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: the lines of a diff never move
              key={i}
              {...stylex.props(
                styles.line,
                line.kind === "add" && styles.lineAdd,
                line.kind === "del" && styles.lineDel,
                line.kind === "hunk" && styles.lineHunk,
                line.kind === "note" && styles.lineNote,
              )}
            >
              <span {...stylex.props(styles.number)}>{line.before ?? ""}</span>
              <span {...stylex.props(styles.number)}>{line.after ?? ""}</span>
              <span {...stylex.props(styles.sign)}>{line.kind === "add" ? "+" : line.kind === "del" ? "−" : ""}</span>
              <span {...stylex.props(styles.text)}>{line.text || " "}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/** The text of a change: of a commit, or without one of the work folder; of one file when chosen. */
function Diff({
  project,
  commit,
  path,
  range = null,
}: {
  project: string;
  commit: string | null;
  path: string | null;
  range?: Range | null;
}) {
  const diff = useDiff(project, commit, path, range);
  const files = useMemo(() => parseDiff(diff.data?.text ?? ""), [diff.data?.text]);
  if (diff.isPending) return <Loading rows={4} />;
  if (diff.error) return <Problem>{`The change did not load: ${failureOf(diff.error)}`}</Problem>;
  if (files.length === 0) return <Hint>There is no text to show for this change.</Hint>;
  return (
    <>
      {diff.data?.truncated ? (
        <Hint>This change is very large: only its first part is here. Choose one file to see that one whole.</Hint>
      ) : null}
      {files.map((file) => (
        <FileDiff key={`${file.from ?? ""}|${file.path}`} file={file} />
      ))}
    </>
  );
}

function WorkingChanges({
  project,
  changes,
  truncated,
  path,
  go,
}: {
  project: string;
  changes: GitChange[];
  truncated: boolean;
  path: string | null;
  go: (next: ChangesSearch) => void;
}) {
  if (changes.length === 0) {
    return (
      <div {...stylex.props(styles.center)}>
        <Empty
          title="Nothing changed since the last commit"
          description="What employees change in this folder shows here until it is committed."
        />
      </div>
    );
  }
  return (
    <div {...stylex.props(styles.body)}>
      <div {...stylex.props(styles.list)}>
        <div {...stylex.props(styles.listHead)}>
          <span {...stylex.props(styles.quiet)}>{changesLine(changes, truncated)}</span>
          {path ? <Button size="sm" label="Show all files" onClick={() => go({})} /> : null}
        </div>
        <div {...stylex.props(styles.listScroll)}>
          {changes.map((change) => (
            <ChangeRow
              key={change.path}
              change={change}
              chosen={path === change.path}
              onClick={() => go(path === change.path ? {} : { path: change.path })}
            />
          ))}
        </div>
      </div>
      <div {...stylex.props(styles.detail)}>
        <div {...stylex.props(styles.detailPad)}>
          <Diff project={project} commit={null} path={path} />
        </div>
      </div>
    </div>
  );
}

function CommitRow({ commit, chosen, onClick }: { commit: GitCommit; chosen: boolean; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} {...stylex.props(styles.row, chosen && styles.rowChosen)}>
      <span {...stylex.props(styles.column)}>
        <span {...stylex.props(styles.truncate)}>{commit.subject || "(no message)"}</span>
        <span {...stylex.props(styles.quiet, styles.truncate)}>
          {commit.author} · {ago(commit.at_ms)} · <span {...stylex.props(styles.code)}>{shortId(commit.id)}</span>
          {commit.parents.length > 1 ? " · merge" : ""}
        </span>
        {commit.refs && commit.refs.length > 0 ? (
          <span {...stylex.props(styles.chips)}>
            {commit.refs.map((ref) => (
              <span key={ref} {...stylex.props(styles.chip)}>
                {ref}
              </span>
            ))}
          </span>
        ) : null}
      </span>
    </button>
  );
}

function CommitDetail({
  project,
  id,
  path,
  go,
}: {
  project: string;
  id: string;
  path: string | null;
  go: (path: string | null) => void;
}) {
  const connection = useConnection();
  const platform = usePlatform();
  const detail = useKeyed(
    ["git", "commit", project, id],
    async () => expectBody(await connection.call("git.commit", { project_id: project, id }), "git_commit").value,
  );
  if (detail.isPending) return <Loading rows={4} />;
  if (detail.error) return <Problem>{`The commit did not load: ${failureOf(detail.error)}`}</Problem>;
  const { commit, body, files, files_truncated: truncated } = detail.data;
  return (
    <>
      <div {...stylex.props(styles.title)}>{commit.subject || "(no message)"}</div>
      {body ? <pre {...stylex.props(styles.message)}>{body}</pre> : null}
      <div {...stylex.props(styles.meta)}>
        <span>
          {commit.author} &lt;{commit.email}&gt;
        </span>
        <span>{new Date(commit.at_ms).toLocaleString("en-GB")}</span>
        <span {...stylex.props(styles.code)}>{shortId(commit.id)}</span>
        <Button size="sm" label="Copy id" onClick={() => void platform.copy(commit.id).then(() => toast("Copied."))} />
        {commit.parents.length > 1 ? <span>A merge: shown is what it brought into its first parent.</span> : null}
      </div>
      <section {...stylex.props(styles.file)}>
        <div {...stylex.props(styles.fileHead)}>
          <span {...stylex.props(styles.column)}>
            {files.length}
            {truncated ? "+" : ""} {files.length === 1 ? "file" : "files"} changed
          </span>
          {path ? <Button size="sm" label="Show all files" onClick={() => go(null)} /> : null}
        </div>
        {files.map((file) => (
          <ChangeRow
            key={file.path}
            change={file}
            chosen={path === file.path}
            onClick={() => go(path === file.path ? null : file.path)}
          />
        ))}
      </section>
      <Diff project={project} commit={commit.id} path={path} />
    </>
  );
}

function Commits({
  project,
  branches,
  current,
  search,
  go,
}: {
  project: string;
  branches: GitBranch[];
  current: string | null;
  search: ChangesSearch;
  go: (next: ChangesSearch) => void;
}) {
  const connection = useConnection();
  const { status, user } = useLinkState();
  const rev = search.rev ?? null;
  // A ticket's branch against the one it was made from: only what the ticket did.
  const base = rev ? (search.base ?? null) : null;
  const log = useInfiniteQuery({
    queryKey: ["git", "log", project, rev, base],
    enabled: status === "online" && user !== null,
    staleTime: 30_000,
    initialPageParam: 0,
    queryFn: async ({ pageParam }) =>
      expectBody(
        await connection.call("git.log", {
          project_id: project,
          ...(rev ? { rev } : {}),
          ...(base ? { base } : {}),
          skip: pageParam,
        }),
        "git_log",
      ).value,
    getNextPageParam: (last, pages) => (last.has_more ? pages.reduce((n, p) => n + p.commits.length, 0) : undefined),
  });
  const commits = log.data?.pages.flatMap((p) => p.commits) ?? [];
  const chosen = search.commit ?? null;
  const keep = rev ? { rev, ...(base ? { base } : {}) } : {};
  return (
    <div {...stylex.props(styles.body)}>
      <div {...stylex.props(styles.list)}>
        <div {...stylex.props(styles.listHead)}>
          <Selector
            label="Branch"
            size="sm"
            value={rev ?? HEAD}
            onChange={(v) => go(v === HEAD ? { view: "commits" } : { view: "commits", rev: v })}
            options={[
              { value: HEAD, label: current ? `${current} (the work folder)` : "The work folder" },
              ...branches.filter((b) => !b.current).map((b) => ({ value: b.name, label: b.name })),
            ]}
          />
          {rev && base ? (
            <>
              <span {...stylex.props(styles.quiet)}>Only what this branch has that {base} has not.</span>
              <Button size="sm" label="Show its whole history" onClick={() => go({ view: "commits", rev })} />
              {chosen ? (
                <Button size="sm" label="Show everything it changed" onClick={() => go({ view: "commits", ...keep })} />
              ) : null}
            </>
          ) : null}
        </div>
        <div {...stylex.props(styles.listScroll)}>
          {log.isPending ? <Loading rows={4} /> : null}
          {log.error ? <Problem>{`The commits did not load: ${failureOf(log.error)}`}</Problem> : null}
          {log.data && commits.length === 0 ? (
            <div {...stylex.props(styles.listHead, styles.quiet)}>No commits yet.</div>
          ) : null}
          {commits.map((commit) => (
            <CommitRow
              key={commit.id}
              commit={commit}
              chosen={chosen === commit.id}
              onClick={() => go({ view: "commits", ...keep, commit: commit.id })}
            />
          ))}
          {log.hasNextPage ? (
            <div {...stylex.props(styles.more)}>
              <Button
                size="sm"
                label={log.isFetchingNextPage ? "Loading…" : "Older commits"}
                isDisabled={log.isFetchingNextPage}
                onClick={() => void log.fetchNextPage()}
              />
            </div>
          ) : null}
        </div>
      </div>
      <div {...stylex.props(styles.detail)}>
        {chosen ? (
          <div {...stylex.props(styles.detailPad)}>
            <CommitDetail
              key={chosen}
              project={project}
              id={chosen}
              path={search.path ?? null}
              go={(path) => go({ view: "commits", ...keep, commit: chosen, ...(path ? { path } : {}) })}
            />
          </div>
        ) : rev && base ? (
          <div {...stylex.props(styles.detailPad)}>
            <div {...stylex.props(styles.title)}>
              Everything {rev} changed since {base}
            </div>
            <span {...stylex.props(styles.meta)}>
              {commits.length === 0 && log.data
                ? "Nothing is committed on this branch yet."
                : "All its commits as one change. Choose a commit on the left to see that one alone."}
            </span>
            <Diff project={project} commit={null} path={null} range={{ rev, base }} />
          </div>
        ) : (
          <div {...stylex.props(styles.center)}>
            <Hint>Choose a commit to see what it changed.</Hint>
          </div>
        )}
      </div>
    </div>
  );
}

function Branches({ branches, go }: { branches: GitBranch[]; go: (next: ChangesSearch) => void }) {
  if (branches.length === 0) {
    return (
      <div {...stylex.props(styles.center)}>
        <Hint>No branches yet: the repository has no commit.</Hint>
      </div>
    );
  }
  return (
    <div {...stylex.props(styles.detail)}>
      <div {...stylex.props(styles.branches)}>
        {branches.map((branch) => (
          <button
            key={`${branch.remote ? "r" : "l"}:${branch.name}`}
            type="button"
            title="Show its commits"
            onClick={() => go(branch.current ? { view: "commits" } : { view: "commits", rev: branch.name })}
            {...stylex.props(styles.branch)}
          >
            <span {...stylex.props(styles.branchName)}>
              {branch.name}
              {branch.current ? <span {...stylex.props(styles.quiet)}> · the work folder</span> : null}
            </span>
            <span {...stylex.props(styles.column)}>
              <span {...stylex.props(styles.truncate)}>{branch.subject || "(no message)"}</span>
              <span {...stylex.props(styles.quiet, styles.truncate)}>
                {[
                  `${branch.author} · ${ago(branch.at_ms)}`,
                  trackLine(branch),
                  branch.worktree ? `checked out in ${branch.worktree}` : "",
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            </span>
            <span {...stylex.props(styles.count)}>{shortId(branch.head)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/** Under All projects: a project is chosen first, since a repository belongs to one. */
function ChooseProject() {
  const projects = useProjects();
  const { choose } = useWorkspace();
  const withFolder = (projects.data ?? []).filter((p) => p.folder);
  return (
    <div {...stylex.props(styles.pick)}>
      <Hint>Changes are per project. Choose one:</Hint>
      {projects.isPending ? <Loading rows={2} /> : null}
      {withFolder.map((p) => (
        <Button key={p.id} label={p.name} onClick={() => choose(p.id)} />
      ))}
      {projects.data && withFolder.length === 0 ? <Hint>No project has a folder on this server yet.</Hint> : null}
    </div>
  );
}

export function ChangesScreen({ search }: { search: ChangesSearch }) {
  const { features } = useLinkState();
  const workspace = useWorkspace();
  const client = useQueryClient();
  const navigate = useNavigate();
  const supported = features.includes(GIT_FEATURE);
  const project = supported ? workspace.id : null;
  const git = useGit(project);
  const view: ChangesView = search.view ?? "changes";
  const go = (next: ChangesSearch) => void navigate({ to: "/changes", search: { ...next } });
  const overview = git.data;
  const count = overview?.changes.length ?? 0;
  return (
    <Page>
      <PageHead
        title="Changes"
        sub={
          workspace.project && overview?.repository
            ? `${workspace.project.name} · ${overview.branch ?? (overview.head ? `at ${shortId(overview.head)}` : "no commit yet")}`
            : workspace.project?.name
        }
        tabs={
          overview?.repository ? (
            <SegmentedControl
              label="What to show"
              size="sm"
              value={view}
              onChange={(v) => go(v === "changes" ? {} : { view: v as ChangesView })}
            >
              <SegmentedControlItem value="changes" label={count > 0 ? `Not committed (${count})` : "Not committed"} />
              <SegmentedControlItem value="commits" label="Commits" />
              <SegmentedControlItem value="branches" label="Branches" />
            </SegmentedControl>
          ) : null
        }
      >
        {project ? (
          <Button
            size="sm"
            label={git.isFetching ? "Refreshing…" : "Refresh"}
            isDisabled={git.isFetching}
            onClick={() => void client.invalidateQueries({ queryKey: ["git"] })}
          />
        ) : null}
      </PageHead>
      {!supported ? (
        <div {...stylex.props(styles.pick)}>
          <Hint>This server does not read git yet. Update the server to see a project's changes here.</Hint>
        </div>
      ) : project === null ? (
        <ChooseProject />
      ) : git.isPending ? (
        <Loading rows={4} />
      ) : git.error ? (
        <div {...stylex.props(styles.pick)}>
          <Problem>{`The repository did not load: ${failureOf(git.error)}`}</Problem>
        </div>
      ) : !overview?.repository ? (
        <div {...stylex.props(styles.center)}>
          <Empty
            title={
              overview?.folder
                ? "This project's folder is not a git repository"
                : "This project has no folder on the server"
            }
            description={
              overview?.folder
                ? `${overview.folder} has no git history. Once it is a repository, its changes and commits show here.`
                : "Give it a folder under Workspace settings."
            }
          />
        </div>
      ) : view === "commits" ? (
        <Commits
          project={project}
          branches={overview.branches}
          current={overview.branch ?? null}
          search={search}
          go={go}
        />
      ) : view === "branches" ? (
        <Branches branches={overview.branches} go={go} />
      ) : (
        <WorkingChanges
          project={project}
          changes={overview.changes}
          truncated={overview.changes_truncated === true}
          path={search.path ?? null}
          go={go}
        />
      )}
    </Page>
  );
}
