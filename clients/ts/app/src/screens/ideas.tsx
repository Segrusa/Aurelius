import { Button } from "@astryxdesign/core/Button";
import { List, ListItem } from "@astryxdesign/core/List";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import { ProtocolError } from "@aurelius/client";
import type { IdeaKind, IdeaView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useIdeas, useProjects } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { AttachRow, useAttachments } from "../ui/attach";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { Held } from "./dialogs";
import { type Author, admits, canBecomeTicket, ideaPrefill, ideasEmpty, splitIdeas } from "./knowledge-logic";
import {
  Confirm,
  DialogSelector,
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
import { overlays } from "./overlays-store";
import { FileChips } from "./run-tools";

const PAGE = 100;
const TITLE_LIMIT = 160;
const NOTE_LIMIT = 8000;
const ALL_PROJECTS = "all";

const styles = stylex.create({
  dialogBody: { display: "flex", flexDirection: "column", gap: 12 },
  kind: {
    display: "inline-flex",
    height: 20,
    minWidth: 58,
    flexShrink: 0,
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
  dot: { width: 6, height: 6, borderRadius: "50%" },
  bug: { backgroundColor: "var(--color-text-error)" },
  feature: { backgroundColor: "var(--color-text-info)" },
  filters: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 6,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
    paddingInline: 18,
    paddingBlock: 10,
  },
  separator: { width: 1, height: 16, marginInline: 4, backgroundColor: "var(--color-border)" },
  split: { display: "flex", minHeight: 0, flex: 1 },
  list: {
    minWidth: 0,
    flexGrow: 999,
    flexShrink: 1,
    flexBasis: 520,
    overflowY: "auto",
    borderInlineEndWidth: 1,
    borderInlineEndStyle: "solid",
    borderInlineEndColor: "var(--color-border)",
  },
  note: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  author: {
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    gap: 6,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  more: { margin: 12 },
  detail: {
    display: "flex",
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: 400,
    flexDirection: "column",
    gap: 14,
    overflowY: "auto",
    paddingInline: 22,
    paddingBlock: 20,
  },
  byline: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  title: { margin: 0, fontSize: 18, lineHeight: 1.35, fontWeight: 600, letterSpacing: "-0.01em" },
  noteText: {
    margin: 0,
    fontSize: 14,
    lineHeight: 1.65,
    whiteSpace: "pre-wrap",
    color: "var(--color-text-secondary)",
  },
  link: { color: "var(--color-text-accent)", textDecoration: "none" },
  actions: { display: "flex", flexWrap: "wrap", gap: 8 },
});

function IdeaDialog({
  idea,
  projectId,
  onClose,
}: {
  idea: IdeaView | null;
  projectId: string | null;
  onClose: () => void;
}) {
  useKeyContext("AureliusDialog", true);
  const connection = useConnection();
  const client = useQueryClient();
  const projects = useProjects();
  const list = (projects.data ?? []).filter((p) => p.status !== "archived");
  const attach = useAttachments(idea?.attachments?.length ?? 0);
  const [dropped, setDropped] = useState<string[]>([]);
  const [title, setTitle] = useState(idea?.title ?? "");
  const [note, setNote] = useState(idea?.note ?? "");
  const [kind, setKind] = useState<IdeaKind>(idea?.kind ?? "feature");
  const [project, setProject] = useState(idea?.project_id ?? projectId ?? list[0]?.id ?? "");
  const [held, setHeld] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const save = async (confirmSecret = false) => {
    if (busy) return;
    if (title.trim() === "") return setError("Enter a name.");
    if (!project) return setError("Make a project first: ideas belong to a project.");
    setBusy(true);
    try {
      const fields = {
        title: title.trim(),
        note: note.trim() === "" ? null : note,
        kind,
        confirm_secret: confirmSecret,
      };
      if (idea)
        await connection.write(
          "idea.update",
          { idea_id: idea.id, ...fields, add_files: attach.paths, remove_files: dropped },
          { retry: false },
        );
      else
        await connection.write("idea.add", { project_id: project, ...fields, files: attach.paths }, { retry: false });
      await client.invalidateQueries({ queryKey: ["ideas"] });
      onClose();
    } catch (e) {
      if (e instanceof ProtocolError && e.code === "looks_like_secret") setHeld(e.matchedItems);
      else setError(failureOf(e));
      setBusy(false);
    }
  };
  useAction("ConfirmPrimary", () => void save());
  const name = idea ? "Edit idea" : "New idea";
  return (
    <FormDialog
      title={name}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label={idea ? "Save" : "Add idea"} isDisabled={busy} onClick={() => void save()} />
        </>
      }
    >
      <div {...attach.target} {...stylex.props(styles.dialogBody)}>
        <Hint>
          For later. When its time comes, make a ticket from it (Ctrl+T in Ideas). Enter adds a line; Ctrl+Enter saves.
        </Hint>
        <TextInput
          hasAutoFocus
          label="Idea"
          placeholder="The idea, for example 'Dark mode'"
          value={title}
          onChange={(v) => setTitle(v.slice(0, TITLE_LIMIT))}
        />
        <TextArea label="Note" rows={4} value={note} onChange={(v) => setNote(v.slice(0, NOTE_LIMIT))} />
        <SegmentedControl label="Kind" size="sm" value={kind} onChange={(v) => setKind(v as IdeaKind)}>
          <SegmentedControlItem value="feature" label="Feature" />
          <SegmentedControlItem value="bug" label="Bug" />
        </SegmentedControl>
        {idea ? null : (
          <DialogSelector
            label="Project"
            value={project}
            onChange={setProject}
            options={list.map((p) => ({ value: p.id, label: p.name }))}
          />
        )}
        {(idea?.attachments ?? [])
          .filter((f) => !dropped.includes(f.path))
          .map((f) => (
            <div key={f.path}>
              <Button size="sm" label={`Remove ${f.name}`} onClick={() => setDropped([...dropped, f.path])}>
                {f.name} ×
              </Button>
            </div>
          ))}
        <AttachRow attach={attach} />
        {held ? <Held names={held} busy={busy} onSend={() => void save(true)} /> : null}
        {error ? <Problem>{error}</Problem> : null}
      </div>
    </FormDialog>
  );
}

const Kind = ({ kind }: { kind: IdeaKind }) => (
  <span {...stylex.props(styles.kind)}>
    <span {...stylex.props(styles.dot, kind === "bug" ? styles.bug : styles.feature)} />
    {kind === "bug" ? "Bug" : "Feature"}
  </span>
);

export function IdeasBody({ projectId, chrome = true }: { projectId: string | null; chrome?: boolean }) {
  const connection = useConnection();
  const client = useQueryClient();
  const { features } = useLinkState();
  const ideas = useIdeas(projectId);
  const projects = useProjects();
  const [status, setStatus] = useState<"open" | "done" | "dropped">("open");
  const [author, setAuthor] = useState<Author>("anyone");
  const [kind, setKind] = useState<"all" | IdeaKind>("all");
  const [project, setProject] = useState<string>("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [limit, setLimit] = useState(PAGE);
  const [dialog, setDialog] = useState<IdeaView | "new" | null>(null);
  const [deleting, setDeleting] = useState<IdeaView | null>(null);
  const all = ideas.data ?? [];
  const inScope = useMemo(() => all.filter((i) => project === "all" || i.project_id === project), [all, project]);
  const openCount = inScope.filter((i) => i.status === "open" && admits(author, i)).length;
  const shown = useMemo(() => {
    const s = splitIdeas(
      inScope.filter((i) => i.status === status && admits(author, i) && (kind === "all" || i.kind === kind)),
    );
    return [...s.yours, ...s.employees];
  }, [inScope, status, author, kind]);
  const selected = shown.find((i) => i.id === selectedId) ?? shown[0] ?? null;
  const write = async (run: () => Promise<unknown>, done?: string) => {
    try {
      await run();
      await client.invalidateQueries({ queryKey: ["ideas"] });
      if (done) toast(done);
    } catch (e) {
      toast(failureOf(e));
    }
  };
  const setIdeaStatus = (i: IdeaView, to: "open" | "done" | "dropped", done?: string) =>
    write(() => connection.write("idea.set_status", { idea_id: i.id, status: to }, { retry: false }), done);
  const makeTicket = (i: IdeaView) => {
    const start = ideaPrefill(i);
    overlays.open("ticket", {
      title: start.title,
      description: start.description,
      labels: start.labels,
      fromIdea: i.id,
      projectId: i.project_id,
    });
  };
  const canTicket = (i: IdeaView) => features.includes("ticket_from_idea") && canBecomeTicket(i);
  useAction("NewItem", () => setDialog("new"));
  useAction("EditItem", () => selected && setDialog(selected), selected !== null);
  useAction(
    "RemoveMemory",
    () =>
      selected?.status === "open" &&
      void setIdeaStatus(selected, "dropped", `Dropped “${selected.title}” (filter 3 shows dropped ideas).`),
    selected !== null,
  );
  useAction("MakeTicket", () => selected && canTicket(selected) && makeTicket(selected), selected !== null);
  useAction("Filter1", () => setStatus("open"));
  useAction("Filter2", () => setStatus("done"));
  useAction("Filter3", () => setStatus("dropped"));
  const nameOf = (id: string) => projects.data?.find((p) => p.id === id)?.name ?? "Unknown project";
  const filters = (
    <div {...stylex.props(styles.filters)}>
      <ToggleButtonGroup label="Status" size="sm" value={status} onChange={(v) => v && setStatus(v as typeof status)}>
        <ToggleButton value="open" label={`Open · ${openCount}`} />
        <ToggleButton value="done" label="Done" />
        <ToggleButton value="dropped" label="Dropped" />
      </ToggleButtonGroup>
      <span {...stylex.props(styles.separator)} />
      <ToggleButtonGroup label="Author" size="sm" value={author} onChange={(v) => v && setAuthor(v as Author)}>
        <ToggleButton value="anyone" label="Everyone" />
        <ToggleButton value="you" label="Yours" />
        <ToggleButton value="employees" label="From employees" />
      </ToggleButtonGroup>
      <span {...stylex.props(styles.separator)} />
      <ToggleButtonGroup label="Kind" size="sm" value={kind} onChange={(v) => v && setKind(v as typeof kind)}>
        <ToggleButton value="all" label="All" />
        <ToggleButton value="bug" label="Bugs" />
        <ToggleButton value="feature" label="Features" />
      </ToggleButtonGroup>
      {projectId ? null : (
        <Selector
          label="Project"
          isLabelHidden
          size="sm"
          value={project}
          onChange={setProject}
          options={[
            { value: ALL_PROJECTS, label: "All projects" },
            ...(projects.data ?? []).map((p) => ({ value: p.id, label: p.name })),
          ]}
        />
      )}
    </div>
  );
  return (
    <Page>
      {chrome ? (
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
          <Button size="sm" variant="primary" label="New idea" onClick={() => setDialog("new")} />
        </PageHead>
      ) : null}
      {filters}
      <div {...stylex.props(styles.split)}>
        <section aria-label="Ideas" {...stylex.props(styles.list)}>
          {ideas.isPending ? <Loading /> : null}
          {ideas.data && shown.length === 0 ? <p {...stylex.props(styles.note)}>{ideasEmpty(status, author)}</p> : null}
          <List hasDividers density="compact">
            {shown.slice(0, limit).map((i) => (
              <ListItem
                key={i.id}
                isSelected={i.id === selected?.id}
                onClick={() => setSelectedId(i.id)}
                align="start"
                startContent={<Kind kind={i.kind ?? "feature"} />}
                label={i.title}
                {...(i.note ? { description: i.note.split("\n")[0] } : {})}
                endContent={
                  <span {...stylex.props(styles.author)}>
                    <Mark letter={initial(i.author)} color={markColor(i.author)} size={16} radius={4} />
                    {i.author}
                    <span>{hhmm(i.created_at_ms)}</span>
                  </span>
                }
              />
            ))}
          </List>
          {shown.length > limit ? (
            <div {...stylex.props(styles.more)}>
              <Button size="sm" label="Show more" onClick={() => setLimit(limit + PAGE)} />
            </div>
          ) : null}
        </section>
        {selected ? (
          <aside aria-label="Idea" {...stylex.props(styles.detail)}>
            <div {...stylex.props(styles.byline)}>
              <Kind kind={selected.kind ?? "feature"} />
              {selected.author === "You" ? "You" : `Idea from ${selected.author}`} · {nameOf(selected.project_id)} ·{" "}
              {hhmm(selected.created_at_ms)}
            </div>
            <h2 {...stylex.props(styles.title)}>{selected.title}</h2>
            {selected.note ? <p {...stylex.props(styles.noteText)}>{selected.note}</p> : null}
            <FileChips files={selected.attachments ?? []} />
            {selected.ticket_id ? (
              <Link to="/board" search={{ ticket: selected.ticket_id }} {...stylex.props(styles.link)}>
                Open ticket{selected.ticket_number ? ` #${selected.ticket_number}` : ""}
              </Link>
            ) : null}
            {selected.task_id ? (
              <Link to="/tasks/$taskId" params={{ taskId: selected.task_id }} {...stylex.props(styles.link)}>
                task{selected.task_status ? ` ${selected.task_status}` : ""}
              </Link>
            ) : null}
            <div {...stylex.props(styles.actions)}>
              {canTicket(selected) ? (
                <Button variant="primary" label="Create ticket…" onClick={() => makeTicket(selected)} />
              ) : null}
              <Button label="Edit…" onClick={() => setDialog(selected)} />
              {selected.status === "open" ? (
                <>
                  <Button label="Mark done" onClick={() => void setIdeaStatus(selected, "done")} />
                  <Button
                    variant="ghost"
                    label="Drop"
                    onClick={() =>
                      void setIdeaStatus(
                        selected,
                        "dropped",
                        `Dropped “${selected.title}” (filter 3 shows dropped ideas).`,
                      )
                    }
                  />
                </>
              ) : (
                <Button label="Reopen" onClick={() => void setIdeaStatus(selected, "open")} />
              )}
              {selected.status === "dropped" && features.includes("idea_attachments") ? (
                <Button variant="destructive" label="Delete…" onClick={() => setDeleting(selected)} />
              ) : null}
            </div>
          </aside>
        ) : null}
      </div>
      {dialog ? (
        <IdeaDialog
          idea={dialog === "new" ? null : dialog}
          projectId={projectId ?? (project === "all" ? null : project)}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {deleting ? (
        <Confirm
          danger
          title={`Delete “${deleting.title}”?`}
          body="The idea and its files are erased; Drop keeps it instead. A task it became stays, with its own copies of the files. This can't be undone."
          action="Delete"
          onClose={() => setDeleting(null)}
          onConfirm={() => {
            const gone = deleting;
            setDeleting(null);
            void write(() => connection.write("idea.delete", { idea_id: gone.id }, { retry: false }));
          }}
        />
      ) : null}
    </Page>
  );
}
