// New session (ticket #60): one page to start work by typing. The rules are in new-session-logic.ts;
// every start is the `ticket.create` New ticket, Give work or their quick task sends, or a `work_session.start`.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useMyTickets, useProjects } from "../data";
import { showChord } from "../keys/keymap";
import { isMac, useAction, useKeyContext, useKeys } from "../keys/provider";
import { usePlatform } from "../platform";
import { usePromptAttachments } from "../ui/attach";
import { CHAT_FILES_FEATURE, withFolders } from "../ui/attach-logic";
import { Chip } from "../ui/chip";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { PromptBox, PromptTextarea } from "../ui/prompt-box";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { cycle, EFFORTS, MODES, titleOfGoal } from "./dialog-logic";
import { Held, useModels, useOpenCreated, useSend } from "./dialogs";
import {
  columnLabel,
  composerRows,
  continueTickets,
  createDrafts,
  DISPATCHER,
  defaultKind,
  defaultProject,
  defaultWho,
  offeredKind,
  parseRecent,
  placeholderOf,
  projectRows,
  remembered,
  SESSION_DISPATCH_FEATURE,
  type SessionKind,
  sessionDispatchParams,
  sessionKey,
  sessionRequest,
  sessionStartParams,
  shownProjects,
  startedSession,
  startProblem,
} from "./new-session-logic";
import { Empty, Loading, Page, PageHead } from "./org-kit";
import { overlays } from "./overlays-store";
import { LAST_QUICK, quickEnabled } from "./quick-logic";
import { ticketPlace } from "./ticket-session-logic";
import { NoConnectionNote, useNoConnection } from "./tickets-kit";

const NARROW = "@media (max-width: 720px)";

const styles = stylex.create({
  body: { display: "flex", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", flexDirection: "column" },
  centre: {
    display: "flex",
    minHeight: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    overflowY: "auto",
    padding: 24,
  },
  centreBlock: { display: "flex", width: "100%", maxWidth: 320, flexDirection: "column", gap: 12 },
  heading: { margin: 0, textAlign: "center", fontSize: 18, fontWeight: 500, color: "var(--color-text-secondary)" },
  card: {
    display: "flex",
    flexDirection: "column",
    borderRadius: "var(--radius-container)",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingBlock: 4,
  },
  // The listbox keeps the focus itself and shows it; its rows are pointed at by aria-activedescendant.
  list: {
    display: "flex",
    flexDirection: "column",
    borderRadius: "var(--radius-container)",
    outline: { default: "none", ":focus-visible": "2px solid var(--color-text-accent)" },
  },
  continueCard: { display: { default: "flex", [NARROW]: "none" } },
  row: {
    display: "flex",
    minWidth: 0,
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 10,
    paddingBlock: 6,
    textAlign: "start",
    fontSize: 13,
    color: "var(--color-text-primary)",
  },
  rowActive: { backgroundColor: "var(--color-overlay-hover)" },
  rowName: { minWidth: 0, flexGrow: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  check: { color: "var(--color-text-accent)" },
  divider: { height: 1, marginBlock: 4, backgroundColor: "var(--color-border)" },
  more: { color: "var(--color-text-secondary)" },
  pill: {
    flexShrink: 0,
    borderRadius: 999,
    paddingInline: 6,
    fontSize: 11,
    color: "var(--precision-text3)",
    backgroundColor: "var(--color-overlay-hover)",
  },
  muted: { color: "var(--precision-text3)" },
  bottom: {
    display: "flex",
    width: "100%",
    maxWidth: 760,
    flexDirection: "column",
    gap: 8,
    marginInline: "auto",
    paddingInline: 16,
    paddingBlockEnd: 24,
  },
  noteLine: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8, fontSize: 12 },
  note: { marginInlineEnd: "auto", color: "var(--precision-text3)" },
  textButton: {
    flexShrink: 0,
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: "var(--color-text-accent)",
  },
  right: { marginInlineStart: "auto" },
});

/** Kept while the app runs, per server: text typed here survives going elsewhere and back. */
const drafts = createDrafts();

/** Asks a shown New session to put the cursor in its composer; a new one does so when it opens. */
const focusListeners = new Set<() => void>();

/** Opens New session with the composer focused: the sidebar's + and the general New. */
export function useOpenNewSession(): () => void {
  const navigate = useNavigate();
  return () => {
    void navigate({ to: "/new" });
    for (const focus of focusListeners) focus();
  };
}

export function NewSessionScreen() {
  useKeyContext("NewSession");
  const connection = useConnection();
  const client = useQueryClient();
  const keys = useKeys();
  const navigate = useNavigate();
  const { storage } = usePlatform();
  const { user, features, hostId, status } = useLinkState();
  const offline = status !== "online";
  const noConnection = useNoConnection();
  const workspace = useWorkspace();
  const projects = useProjects();
  const employees = useEmployees();
  const models = useModels();
  const quickOn = quickEnabled(features);
  const responsible = features.includes("ticket_responsible");
  const mine = useMyTickets(workspace.id, false, features.includes("my_tickets"));

  const composer = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const focus = () => composer.current?.focus();
    focusListeners.add(focus);
    focus();
    return () => void focusListeners.delete(focus);
  }, []);

  const [text, setText] = useState(() => drafts.get(hostId));
  const write = (next: string) => {
    setText(next);
    drafts.set(hostId, next);
  };
  // Next to Ticket: a Session where the server has them (it replaces the quick task), else a Quick task.
  const other = offeredKind(features, quickOn);
  const [chosenKind, setKind] = useState<SessionKind>(() =>
    defaultKind(storage.get(sessionKey("kind", hostId)), true, other === "session"),
  );
  const kind: SessionKind = other && (chosenKind === "ticket" || chosenKind === other) ? chosenKind : "ticket";
  const [worktree, setWorktree] = useState(false);
  // A ticket takes files of any type; a session's first message where the server has `chat_files`.
  const sessionFiles = features.includes(CHAT_FILES_FEATURE);
  const attach = usePromptAttachments({ files: kind !== "session" || (sessionFiles && !worktree), folders: true });
  const [pickedProject, setProject] = useState<string | null>(null);
  const [pickedWho, setWho] = useState<string | null>(null);
  const [model, setModel] = useState("default");
  const [effort, setEffort] = useState("default");
  const [mode, setMode] = useState("default");

  const shown = useMemo(() => shownProjects(projects.data ?? [], workspace.id), [projects.data, workspace.id]);
  const recent = parseRecent(storage.get(sessionKey("recent", hostId)));
  const projectId = shown.some((p) => p.id === pickedProject)
    ? pickedProject
    : defaultProject(shown, workspace.id, storage.get(sessionKey("project", hostId)));
  const project = shown.find((p) => p.id === projectId) ?? null;
  const { rows, more } = projectRows(shown, recent, projectId);

  const team = employees.data ?? [];
  const active = team.filter((e) => e.status === "active");
  // A session goes to the dispatcher too where the server lets it choose (`session_dispatch`).
  const sessionDispatch = features.includes(SESSION_DISPATCH_FEATURE);
  const dispatcherOffered = kind === "ticket" || (kind === "session" && sessionDispatch);
  const fallbackWho = defaultWho(
    kind,
    storage.get(sessionKey("who", hostId)),
    team,
    storage.get(LAST_QUICK),
    sessionDispatch,
  );
  const whoPicked = pickedWho === DISPATCHER ? dispatcherOffered : active.some((e) => e.id === pickedWho);
  const whoId = whoPicked ? pickedWho : fallbackWho;
  const who = active.find((e) => e.id === whoId) ?? null;
  const whoOptions = [
    ...(dispatcherOffered ? [{ key: DISPATCHER, label: "Dispatcher" }] : []),
    ...active.map((e) => ({ key: e.id, label: e.name, description: e.title ?? e.role })),
  ];
  const whoLabel =
    whoId === DISPATCHER
      ? "Dispatcher"
      : (who?.name ?? (active.length === 0 ? "No employee yet" : "Choose who does it"));

  const problem = startProblem({
    projectId,
    text,
    kind,
    who: whoId,
    offline,
    files: attach.files.length,
    sessionFiles,
    worktree,
    sessionDispatch,
  });
  // Lands where the dialogs land since #58: the new ticket's chat, else its panel on the Board.
  const created = useOpenCreated();
  // A started session opens itself; a ticket where the dialogs open one.
  const startedTo = useRef<string | null>(null);
  const { busy, error, held, send, sendAnyway } = useSend((done) => {
    toast(done);
    drafts.clear(hostId);
    setText("");
    attach.clear();
    const session = startedTo.current;
    startedTo.current = null;
    if (session) {
      // The new session is in the list its screen reads, also before the server's events arrive.
      void client.invalidateQueries({ queryKey: ["messages", "work_sessions"] });
      void navigate({ to: "/session/$conversationId", params: { conversationId: session } });
    } else created.open();
  });

  const start = () => {
    if (problem || busy || !project || !whoId) return;
    const startWho = { id: whoId, name: whoId === DISPATCHER ? "Dispatcher" : (who?.name ?? "the employee") };
    send(async (confirmSecret) => {
      if (kind === "session") {
        const first = {
          projectId: project.id,
          text: withFolders(text, attach.folders),
          model,
          effort,
          mode,
          images: attach.images,
          files: attach.files,
        };
        const dispatched = whoId === DISPATCHER;
        const receipt = dispatched
          ? await connection.write("work_session.dispatch", sessionDispatchParams(first, worktree, confirmSecret), {
              retry: false,
            })
          : await connection.write(
              "work_session.start",
              sessionStartParams({ ...first, employeeId: whoId }, worktree, confirmSecret),
              { retry: false },
            );
        startedTo.current = startedSession(receipt);
        for (const [key, value] of remembered(hostId, { kind, projectId: project.id, who: whoId }, recent)) {
          storage.set(key, value);
        }
        return dispatched
          ? "Session started: the dispatcher chooses who takes it."
          : `Session started with ${startWho.name}.`;
      }
      const { params, done } = sessionRequest(
        {
          kind,
          projectId: project.id,
          text: withFolders(text, attach.folders),
          who: startWho,
          model,
          effort,
          mode,
          // A ticket keeps photos as files.
          files: [...attach.images, ...attach.files],
          responsibleUserId: responsible ? (user?.id ?? null) : null,
        },
        confirmSecret,
      );
      const receipt = await connection.write("ticket.create", params, { retry: false });
      created.remember(receipt);
      for (const [key, value] of remembered(hostId, { kind, projectId: project.id, who: whoId }, recent)) {
        storage.set(key, value);
      }
      return done;
    });
  };

  useAction("StartNewSession", start);
  useAction("CycleGiveProject", () => {
    if (shown.length > 0)
      setProject(
        cycle(
          shown.map((p) => p.id),
          projectId ?? "",
        ),
      );
  });
  useAction("CycleTaskManager", () => {
    if (whoOptions.length > 0)
      setWho(
        cycle(
          whoOptions.map((o) => o.key),
          whoId ?? "",
        ),
      );
  });
  useAction("CycleTaskModel", () =>
    setModel(
      cycle(
        models.map((m) => m.key),
        model,
      ),
    ),
  );
  useAction("CycleTaskEffort", () => setEffort(cycle(EFFORTS, effort)));
  useAction(
    "CycleTaskMode",
    () =>
      kind !== "quick" &&
      setMode(
        cycle(
          MODES.map((m) => m.key as string),
          mode,
        ),
      ),
  );

  const pickProject = (id: string) => {
    setProject(id);
    composer.current?.focus();
  };
  const noProjects = projects.isSuccess && shown.length === 0;
  const continuing = continueTickets(mine.data?.tickets ?? []);

  let centre: ReactNode;
  if (offline) {
    centre = (
      <Empty title="Not connected to the server." description="What you type here stays until it is back.">
        <Button label="Try again" onClick={() => connection.wake()} />
      </Empty>
    );
  } else if (projects.isError) {
    centre = (
      <Empty title="The projects did not load." description={projects.error.message}>
        <Button label="Try again" onClick={() => void projects.refetch()} />
      </Empty>
    );
  } else if (!projects.isSuccess) {
    centre = <Loading rows={3} />;
  } else if (noProjects) {
    centre = (
      <Empty title="Create a project first: work always belongs to one.">
        <Button label="New project" variant="primary" onClick={() => keys.run("NewProject")} />
      </Empty>
    );
  } else {
    centre = (
      <>
        <ProjectCard rows={rows} more={more} all={shown} chosen={projectId} onPick={pickProject} />
        {continuing.length > 0 ? (
          <div {...stylex.props(styles.card, styles.continueCard)}>
            {continuing.map((t) => (
              <button
                key={t.id}
                type="button"
                onClick={() => void navigate(ticketPlace(t))}
                {...stylex.props(styles.row)}
              >
                <span aria-hidden="true" {...stylex.props(styles.muted)}>
                  ↻
                </span>
                <span {...stylex.props(styles.rowName)}>
                  Continue: #{t.number} {t.title}
                </span>
                <span {...stylex.props(styles.pill)}>{columnLabel(t.column)}</span>
              </button>
            ))}
          </div>
        ) : null}
      </>
    );
  }

  return (
    <Page>
      <PageHead title="New session" />
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.centre)}>
          <div {...stylex.props(styles.centreBlock)}>
            <Heading level={2} xstyle={styles.heading}>
              What should happen next?
            </Heading>
            {centre}
          </div>
        </div>
        <div {...stylex.props(styles.bottom)}>
          {employees.isError ? (
            <Banner
              status="warning"
              title="The employees did not load: only the dispatcher can get a ticket now."
              endContent={<Button label="Try again" onClick={() => void employees.refetch()} />}
            />
          ) : null}
          {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
          {error ? <Banner status="error" role="alert" title={error} /> : null}
          <PromptBox
            attach={attach}
            input={
              <PromptTextarea
                ref={composer}
                aria-label="What needs doing"
                placeholder={placeholderOf(kind)}
                rows={composerRows(text)}
                value={text}
                onChange={(e) => write(e.target.value)}
                disabled={noProjects}
              />
            }
            chips={
              <>
                <Menu
                  label="Model"
                  value={model}
                  options={models}
                  onPick={setModel}
                  trigger={
                    <Chip name="Model" quiet>
                      {models.find((m) => m.key === model)?.label ?? "Default"}
                    </Chip>
                  }
                />
                <Menu
                  label="Effort"
                  value={effort}
                  options={EFFORTS.map((e) => ({ key: e, label: e === "default" ? "Default" : e }))}
                  onPick={setEffort}
                  trigger={
                    <Chip name="Effort" quiet>
                      {effort === "default" ? "Default" : effort}
                    </Chip>
                  }
                />
                {kind !== "quick" ? (
                  <Menu
                    label="Mode"
                    value={mode}
                    options={MODES.map((m) => ({
                      key: m.key as string,
                      label:
                        m.key === "default" ? (kind === "session" ? "Employee's default" : "Owner's default") : m.label,
                    }))}
                    onPick={setMode}
                    trigger={
                      <Chip name="Mode" quiet>
                        {mode === "default"
                          ? kind === "session"
                            ? "Employee's default"
                            : "Owner's default"
                          : MODES.find((m) => m.key === mode)?.label}
                      </Chip>
                    }
                  />
                ) : null}
              </>
            }
            send={{
              label: `Start (${showChord("secondary-enter", isMac)})`,
              title: problem ?? `Start (${showChord("secondary-enter", isMac)})`,
              disabled: problem !== null || busy,
              onClick: start,
            }}
            tray={
              <>
                <Menu
                  label="Project"
                  {...(projectId ? { value: projectId } : {})}
                  options={shown.map((p) => ({ key: p.id, label: p.name }))}
                  onPick={pickProject}
                  trigger={
                    <Chip name="Project" quiet>
                      {project ? (
                        <>
                          <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={3} />
                          {project.name}
                        </>
                      ) : (
                        "Choose"
                      )}
                    </Chip>
                  }
                />
                {other ? (
                  <SegmentedControl
                    label="Kind"
                    size="sm"
                    value={kind}
                    onChange={(next) => setKind(next === other ? other : "ticket")}
                  >
                    <SegmentedControlItem value="ticket" label="Ticket" />
                    <SegmentedControlItem value={other} label={other === "session" ? "Session" : "Quick task"} />
                  </SegmentedControl>
                ) : null}
                <Menu
                  label="Who"
                  {...(whoId ? { value: whoId } : {})}
                  options={whoOptions}
                  onPick={setWho}
                  trigger={
                    <Chip name="Who" quiet>
                      {whoLabel}
                    </Chip>
                  }
                />
                {kind === "session" ? (
                  <Menu
                    label="Where"
                    value={worktree ? "worktree" : "folder"}
                    options={[
                      { key: "folder", label: "Project folder", description: "Works in the project's folder itself" },
                      { key: "worktree", label: "Own worktree", description: "A worktree and branch of its own" },
                    ]}
                    onPick={(key) => setWorktree(key === "worktree")}
                    trigger={
                      <Chip name="Where" quiet>
                        {worktree ? "Own worktree" : "Project folder"}
                      </Chip>
                    }
                  />
                ) : null}
                {kind === "ticket" && project ? (
                  <button
                    type="button"
                    onClick={() =>
                      overlays.open("ticket", {
                        projectId: project.id,
                        title: titleOfGoal(text),
                        description: text.trim(),
                      })
                    }
                    {...stylex.props(styles.textButton, styles.right)}
                  >
                    More options
                  </button>
                ) : null}
              </>
            }
          />
          {noConnection ? (
            <div {...stylex.props(styles.noteLine)}>
              <span {...stylex.props(styles.note)}>
                <NoConnectionNote />
              </span>
            </div>
          ) : null}
        </div>
      </div>
    </Page>
  );
}

/** §4.1: the projects as a list to pick from; arrows move, Enter picks, and the composer gets the focus back. */
function ProjectCard({
  rows,
  more,
  all,
  chosen,
  onPick,
}: {
  rows: { id: string; name: string }[];
  more: boolean;
  all: { id: string; name: string }[];
  chosen: string | null;
  onPick: (id: string) => void;
}) {
  const [active, setActive] = useState(() =>
    Math.max(
      0,
      rows.findIndex((p) => p.id === chosen),
    ),
  );
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((at) => (at + step + rows.length) % rows.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const row = rows[active];
      if (row) onPick(row.id);
    }
  };
  const activeId = rows[active] ? `new-session-project-${rows[active].id}` : undefined;
  return (
    <div {...stylex.props(styles.card)}>
      <div
        role="listbox"
        aria-label="Project"
        tabIndex={0}
        {...(activeId ? { "aria-activedescendant": activeId } : {})}
        onKeyDown={onKeyDown}
        {...stylex.props(styles.list)}
      >
        {rows.map((p, i) => (
          // biome-ignore lint/a11y/useKeyWithClickEvents: the listbox takes the keys and points at this row (aria-activedescendant).
          <div
            key={p.id}
            id={`new-session-project-${p.id}`}
            role="option"
            aria-selected={p.id === chosen}
            tabIndex={-1}
            onClick={() => onPick(p.id)}
            onMouseEnter={() => setActive(i)}
            {...stylex.props(styles.row, i === active && styles.rowActive)}
          >
            <Mark letter={initial(p.name)} color={markColor(p.id)} size={18} radius={4} />
            <span {...stylex.props(styles.rowName)}>{p.name}</span>
            {p.id === chosen ? (
              <span aria-hidden="true" {...stylex.props(styles.check)}>
                ✓
              </span>
            ) : null}
          </div>
        ))}
      </div>
      {more ? (
        <>
          <div {...stylex.props(styles.divider)} />
          <Menu
            label="All projects"
            {...(chosen ? { value: chosen } : {})}
            options={all.map((p) => ({ key: p.id, label: p.name }))}
            onPick={onPick}
            trigger={
              <button type="button" {...stylex.props(styles.row, styles.more)}>
                All projects…
              </button>
            }
          />
        </>
      ) : null}
    </div>
  );
}
