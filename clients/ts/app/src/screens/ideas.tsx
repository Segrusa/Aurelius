import { ProtocolError } from "@aurelius/client";
import type { IdeaKind, IdeaView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useIdeas, useProjects } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { Button, initial, Mark, markColor, Segments } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { toast } from "../ui/toast";
import { Held } from "./dialogs";
import { type Author, admits, canBecomeTicket, ideaPrefill, ideasEmpty, splitIdeas } from "./knowledge-logic";
import { AREA, Confirm, FIELD, failureOf, hhmm, KnowledgeTabs, PageHead, SMALL } from "./org-kit";
import { overlays } from "./overlays-store";

const PAGE = 100;

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
      if (idea) await connection.write("idea.update", { idea_id: idea.id, ...fields }, { retry: false });
      else await connection.write("idea.add", { project_id: project, ...fields }, { retry: false });
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
    <Dialog open onClose={onClose} title={name} width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">{name}</p>
        <p className="m-0 text-[12px] leading-[1.5] text-text3">
          For later. When its time comes, make a ticket from it (Ctrl+T in Ideas). Enter adds a line; Ctrl+Enter saves.
        </p>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Idea
          <input
            data-autofocus
            maxLength={160}
            placeholder="The idea, for example 'Dark mode'"
            className={FIELD}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-text3">
          Note
          <textarea rows={4} maxLength={8000} className={AREA} value={note} onChange={(e) => setNote(e.target.value)} />
        </label>
        <Segments
          label="Kind"
          value={kind}
          options={[
            { key: "feature", label: "Feature" },
            { key: "bug", label: "Bug" },
          ]}
          onChange={setKind}
        />
        {idea ? null : (
          <label className="flex flex-col gap-1 text-[12px] text-text3">
            Project
            <select className={FIELD} value={project} onChange={(e) => setProject(e.target.value)}>
              {list.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {held ? <Held names={held} busy={busy} onSend={() => void save(true)} /> : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy} onClick={() => void save()}>
          {idea ? "Save" : "Add idea"}
        </Button>
      </div>
    </Dialog>
  );
}

const Kind = ({ kind }: { kind: IdeaKind }) => (
  <span className="inline-flex h-5 min-w-[58px] flex-none items-center gap-[6px] rounded-[10px] border border-line px-[7px] text-[11px] text-text2">
    <span className={`size-[6px] rounded-full ${kind === "bug" ? "bg-red" : "bg-blue"}`} />
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
  useAction("MakeTicket", () => selected && canTicket(selected) && makeTicket(selected), selected !== null);
  useAction("Filter1", () => setStatus("open"));
  useAction("Filter2", () => setStatus("done"));
  useAction("Filter3", () => setStatus("dropped"));
  const pill = (on: boolean) =>
    `h-6 cursor-pointer rounded-[12px] border px-[9px] text-[12px] font-medium ${on ? "border-btn-line bg-sel text-text" : "border-line bg-transparent text-text2"}`;
  const nameOf = (id: string) => projects.data?.find((p) => p.id === id)?.name ?? "Unknown project";
  const filters = (
    <div className="flex flex-wrap items-center gap-[6px] border-b border-line2 px-[18px] py-[10px]">
      {(["open", "done", "dropped"] as const).map((s) => (
        <button
          key={s}
          type="button"
          aria-pressed={status === s}
          className={pill(status === s)}
          onClick={() => setStatus(s)}
        >
          {s === "open" ? `Open · ${openCount}` : s === "done" ? "Done" : "Dropped"}
        </button>
      ))}
      <span className="mx-1 h-4 w-px bg-line" />
      {(["anyone", "you", "employees"] as const).map((a) => (
        <button
          key={a}
          type="button"
          aria-pressed={author === a}
          className={pill(author === a)}
          onClick={() => setAuthor(a)}
        >
          {a === "anyone" ? "Everyone" : a === "you" ? "Yours" : "From employees"}
        </button>
      ))}
      <span className="mx-1 h-4 w-px bg-line" />
      {(["all", "bug", "feature"] as const).map((k) => (
        <button key={k} type="button" aria-pressed={kind === k} className={pill(kind === k)} onClick={() => setKind(k)}>
          {k === "all" ? "All" : k === "bug" ? "Bugs" : "Features"}
        </button>
      ))}
      {projectId ? null : (
        <select
          aria-label="Project"
          value={project}
          onChange={(e) => setProject(e.target.value)}
          className={`${pill(project !== "all")} outline-none`}
        >
          <option value="all">All projects</option>
          {(projects.data ?? []).map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      )}
    </div>
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {chrome ? (
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
          <Button kind="primary" className={SMALL} onClick={() => setDialog("new")}>
            New idea
          </Button>
        </PageHead>
      ) : null}
      {filters}
      <div className="flex min-h-0 flex-1">
        <section aria-label="Ideas" className="min-w-0 flex-[999_1_520px] overflow-y-auto border-r border-line">
          {ideas.isPending ? <p className="m-0 p-6 text-text3">Loading ideas…</p> : null}
          {ideas.data && shown.length === 0 ? <p className="m-0 p-6 text-text3">{ideasEmpty(status, author)}</p> : null}
          {shown.slice(0, limit).map((i) => (
            <button
              key={i.id}
              type="button"
              aria-current={i.id === selected?.id ? "true" : undefined}
              onClick={() => setSelectedId(i.id)}
              className={`flex w-full cursor-pointer items-start gap-3 border-0 border-b border-line2 px-[18px] py-[14px] text-left text-text ${i.id === selected?.id ? "bg-sel shadow-[inset_2px_0_0_var(--acc-text)]" : "bg-transparent hover:bg-sel"}`}
            >
              <Kind kind={i.kind ?? "feature"} />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{i.title}</span>
                {i.note ? (
                  <span className="mt-[3px] block truncate text-[12px] text-text3">{i.note.split("\n")[0]}</span>
                ) : null}
              </span>
              <span className="flex flex-none items-center gap-[6px] text-[12px] text-text3">
                <Mark letter={initial(i.author)} color={markColor(i.author)} size={16} radius={4} />
                {i.author}
                <span>{hhmm(i.created_at_ms)}</span>
              </span>
            </button>
          ))}
          {shown.length > limit ? (
            <Button className={`${SMALL} m-3`} onClick={() => setLimit(limit + PAGE)}>
              Show more
            </Button>
          ) : null}
        </section>
        {selected ? (
          <aside
            aria-label="Idea"
            className="flex min-w-0 flex-[1_1_400px] flex-col gap-[14px] overflow-y-auto px-[22px] py-5"
          >
            <div className="flex flex-wrap items-center gap-2 text-[12px] text-text3">
              <Kind kind={selected.kind ?? "feature"} />
              {selected.author === "You" ? "You" : `Idea from ${selected.author}`} · {nameOf(selected.project_id)} ·{" "}
              {hhmm(selected.created_at_ms)}
            </div>
            <h2 className="m-0 text-[18px] leading-[1.35] font-semibold tracking-[-0.01em]">{selected.title}</h2>
            {selected.note ? (
              <p className="m-0 text-[14px] leading-[1.65] whitespace-pre-wrap text-text2">{selected.note}</p>
            ) : null}
            {selected.ticket_id ? (
              <Link to="/board" search={{ ticket: selected.ticket_id }} className="text-acc-text no-underline">
                Open ticket{selected.ticket_number ? ` #${selected.ticket_number}` : ""}
              </Link>
            ) : null}
            {selected.task_id ? (
              <Link to="/tasks/$taskId" params={{ taskId: selected.task_id }} className="text-acc-text no-underline">
                task{selected.task_status ? ` ${selected.task_status}` : ""}
              </Link>
            ) : null}
            <div className="flex flex-wrap gap-2">
              {canTicket(selected) ? (
                <Button kind="primary" onClick={() => makeTicket(selected)}>
                  Create ticket…
                </Button>
              ) : null}
              <Button onClick={() => setDialog(selected)}>Edit…</Button>
              {selected.status === "open" ? (
                <>
                  <Button onClick={() => void setIdeaStatus(selected, "done")}>Mark done</Button>
                  <Button
                    kind="ghost"
                    onClick={() =>
                      void setIdeaStatus(
                        selected,
                        "dropped",
                        `Dropped “${selected.title}” (filter 3 shows dropped ideas).`,
                      )
                    }
                  >
                    Drop
                  </Button>
                </>
              ) : (
                <Button onClick={() => void setIdeaStatus(selected, "open")}>Reopen</Button>
              )}
              {selected.status === "dropped" && features.includes("idea_attachments") ? (
                <Button kind="danger" onClick={() => setDeleting(selected)}>
                  Delete…
                </Button>
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
    </div>
  );
}
