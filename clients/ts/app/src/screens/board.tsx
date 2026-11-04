import type { BoardColumnView, BoardView, ColumnKind, TicketView } from "@aurelius/protocol";
import {
  closestCorners,
  DndContext,
  type DragEndEvent,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  useDroppable,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useBoard, useEmployees, useProjects } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { NewIcon } from "../ui/icons";
import { Button, Kbd, Segments } from "../ui/kit";
import { Dialog, Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import {
  beforeFor,
  columnShown,
  dropEffect,
  emptyText,
  movedCard,
  type Needs,
  type OwnerFilter,
  shownCards,
} from "./board-logic";
import { Held, useSend } from "./dialogs";
import { overlays } from "./overlays-store";
import { ConfirmDialog, TicketPanel } from "./ticket-panel";
import { TicketRows } from "./ticket-rows";
import {
  ColumnGlyph,
  NeedsTag,
  OwnerMark,
  TicketTags,
  ticketNumber,
  useTicketNeeds,
  useTicketWrite,
} from "./tickets-kit";

const HIDDEN_KEY = "board_hidden_columns";
const CARD_HEIGHT = 88;

const CardBody = memo(function CardBody({
  t,
  need,
  mine,
  selected,
}: {
  t: TicketView;
  need: Needs | null;
  mine: boolean;
  selected: boolean;
}) {
  return (
    <span
      className={`flex w-full flex-col gap-2 rounded-[7px] border bg-card px-3 py-[10px] text-left text-text shadow-card ${selected ? "border-acc-text shadow-[0_0_0_1px_var(--acc-text)]" : "border-line hover:border-btn-line"}`}
    >
      <span className="flex items-center gap-[6px] font-mono text-[12px] text-text3">
        {ticketNumber(t)}
        <span className="ml-auto">
          <OwnerMark t={t} />
        </span>
      </span>
      <span className="leading-[1.4] font-medium">{t.title}</span>
      <span className="flex flex-wrap items-center gap-[6px] empty:hidden">
        {need ? <NeedsTag needs={need} mine={mine} /> : null}
        <TicketTags t={t} />
      </span>
    </span>
  );
});

function Card({
  t,
  kind,
  need,
  mine,
  selected,
  onOpen,
}: {
  t: TicketView;
  kind: ColumnKind;
  need: Needs | null;
  mine: boolean;
  selected: boolean;
  onOpen: (id: string) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: t.id,
    data: { kind },
  });
  return (
    <button
      ref={setNodeRef}
      type="button"
      data-card={t.id}
      data-column={kind}
      aria-label={`${ticketNumber(t)} ${t.title}`}
      onClick={() => onOpen(t.id)}
      {...attributes}
      {...listeners}
      style={{ transform: CSS.Translate.toString(transform), transition, opacity: isDragging ? 0.4 : 1 }}
      className="block w-full cursor-pointer rounded-[7px] border-0 bg-transparent p-0 text-left"
    >
      <CardBody t={t} need={need} mine={mine} selected={selected} />
    </button>
  );
}

function Column({
  column,
  cards,
  older,
  total,
  need,
  mine,
  selected,
  onOpen,
  onShowOlder,
}: {
  column: BoardColumnView;
  cards: TicketView[];
  older: number;
  total: number;
  need: (t: TicketView) => Needs | null;
  mine: (t: TicketView) => boolean;
  selected: string | undefined;
  onOpen: (id: string) => void;
  onShowOlder: () => void;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: `column:${column.kind}`, data: { kind: column.kind } });
  const ids = useMemo(() => cards.map((c) => c.id), [cards]);
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: cards.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => CARD_HEIGHT,
    gap: 6,
    overscan: 6,
    getItemKey: (i) => (cards[i] as TicketView).id,
  });
  const items = virtual.getVirtualItems();
  const top = items[0]?.start ?? 0;
  const bottom = Math.max(0, virtual.getTotalSize() - (items.at(-1)?.end ?? 0));
  return (
    <section
      aria-label={column.name}
      ref={setNodeRef}
      className={`flex max-h-full w-[248px] flex-none flex-col gap-[6px] rounded-[8px] ${isOver ? "bg-sel" : ""}`}
    >
      <div className="flex h-8 flex-none items-center gap-2 px-[6px]">
        <ColumnGlyph kind={column.kind} />
        <span className="font-medium">{column.name}</span>
        <span className="text-text3">{total}</span>
        {column.kind === "backlog" ? (
          <button
            type="button"
            aria-label={`Add ticket to ${column.name}`}
            onClick={() => overlays.open("ticket")}
            className="ml-auto grid size-[22px] cursor-pointer place-items-center rounded-[5px] border-0 bg-transparent text-text3 hover:bg-sel hover:text-text"
          >
            <NewIcon />
          </button>
        ) : null}
      </div>
      <div ref={scroller} className="min-h-[40px] overflow-y-auto pb-1">
        <SortableContext items={ids} strategy={verticalListSortingStrategy}>
          {/* In flow between two paddings: the sortable transforms need the cards where they stand. */}
          <div style={{ paddingTop: top, paddingBottom: bottom }} className="flex flex-col gap-[6px]">
            {items.map((item) => {
              const t = cards[item.index] as TicketView;
              return (
                <div key={t.id} data-index={item.index} ref={virtual.measureElement}>
                  <Card
                    t={t}
                    kind={column.kind}
                    need={need(t)}
                    mine={mine(t)}
                    selected={t.id === selected}
                    onOpen={onOpen}
                  />
                </div>
              );
            })}
          </div>
        </SortableContext>
        {cards.length === 0 ? (
          <p className="m-0 px-[6px] py-1 text-[12px] text-text3">{emptyText(column.kind)}</p>
        ) : null}
        {older > 0 ? (
          <button
            type="button"
            onClick={onShowOlder}
            className="cursor-pointer border-0 bg-transparent p-[6px] text-left text-[12px] text-text3 hover:text-text"
          >
            Show older ({older})
          </button>
        ) : null}
      </div>
    </section>
  );
}

function SendBackDialog({ t, onClose }: { t: TicketView; onClose: () => void }) {
  const connection = useConnection();
  const [feedback, setFeedback] = useState("");
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    toast(text);
    onClose();
  });
  const go = () => {
    if (feedback.trim() === "" || busy) return;
    send(async (confirmSecret) => {
      await connection.write(
        "ticket.send_back",
        { ticket_id: t.id, feedback: feedback.trim(), confirm_secret: confirmSecret },
        { retry: false },
      );
      return `${ticketNumber(t)} went back to Doing.`;
    });
  };
  useAction("ConfirmPrimary", go);
  useKeyContext("AureliusDialog", true);
  return (
    <>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">Send {ticketNumber(t)} back to Doing</p>
        <textarea
          data-autofocus
          aria-label="What needs to change?"
          placeholder="What needs to change?"
          className="min-h-[100px] w-full resize-y rounded-[6px] border border-btn-line bg-sunk p-2 text-text outline-none focus:border-acc-text"
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
        />
        {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy || feedback.trim() === ""} onClick={go}>
          Send back
        </Button>
      </div>
    </>
  );
}

type Pending = { kind: "send_back" | "mark_done" | "back_to_backlog" | "reject"; ticket: TicketView } | null;

export function BoardScreen({ ticket, projectId }: { ticket: string | undefined; projectId?: string }) {
  useKeyContext("Board");
  const navigate = useNavigate();
  const client = useQueryClient();
  const { storage } = usePlatform();
  const { features } = useLinkState();
  const write = useTicketWrite();
  const [chosenProject, setProject] = useState<string | null>(() => storage.get("board_project"));
  const project = projectId ?? chosenProject;
  const [owner, setOwner] = useState<OwnerFilter>({ kind: "anyone" });
  const [query, setQuery] = useState("");
  const [needsOnly, setNeedsOnly] = useState(false);
  const [doneAll, setDoneAll] = useState(false);
  const [view, setView] = useState<"board" | "list">("board");
  const [hidden, setHidden] = useState<ColumnKind[]>(
    () => (storage.get(HIDDEN_KEY)?.split(",").filter(Boolean) ?? []) as ColumnKind[],
  );
  const [dragging, setDragging] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending>(null);
  const projects = useProjects();
  const employees = useEmployees();
  const board = useBoard(project);
  const all = useMemo(() => (board.data?.columns ?? []).flatMap((c) => c.tickets), [board.data]);
  const need = useTicketNeeds(all);
  const needsCount = useMemo(() => all.filter(need.mine).length, [all, need]);
  const testing = features.includes("board_testing");
  const columns = useMemo(
    () => (board.data?.columns ?? []).filter((c) => columnShown(c.column.kind, testing)),
    [board.data, testing],
  );
  const filter = useMemo(() => ({ owner, query, needsOnly, needs: need.mine }), [owner, query, needsOnly, need]);
  const shown = useMemo(
    () => columns.map((c) => ({ ...c, ...shownCards(c.column.kind, c.tickets, filter, doneAll) })),
    [columns, filter, doneAll],
  );
  const open = (id: string) => void navigate({ to: "/board", search: { ticket: id } });
  const close = () => void navigate({ to: "/board", search: {} });
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
      keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space", "Enter"] },
    }),
  );
  const key = ["tickets", "board", project];

  const moveLocally = (id: string, to: ColumnKind, before: string | null) => {
    client.setQueryData<BoardView>(key, (old) => {
      if (!old) return old;
      const next = movedCard(
        old.columns.map((c) => ({ key: c.column.kind as string, tickets: c.tickets })),
        id,
        to,
        before,
      );
      return {
        ...old,
        columns: old.columns.map((c, i) => ({
          ...c,
          tickets: (next[i]?.tickets ?? c.tickets).map((t) => (t.id === id ? { ...t, column: to } : t)),
        })),
      };
    });
  };

  const drop = (event: DragEndEvent) => {
    setDragging(null);
    const t = all.find((x) => x.id === event.active.id);
    if (!t || !event.over) return;
    const overId = String(event.over.id);
    const to = (event.over.data.current?.kind ?? t.column) as ColumnKind;
    const onto = overId.startsWith("column:") ? null : overId;
    const effect = dropEffect(t.column, to, need.choosing(t));
    switch (effect.kind) {
      case "refused":
        toast(effect.why);
        return;
      case "reorder": {
        const column = columns.find((c) => c.column.kind === to);
        if (!column) return;
        const before = beforeFor(column.tickets, t.id, onto);
        if (before === undefined) return;
        moveLocally(t.id, to, before);
        void write("ticket.move", { ticket_id: t.id, column_id: column.column.id, before_ticket_id: before });
        return;
      }
      case "start":
        moveLocally(t.id, to, null);
        void write("ticket.start", { ticket_id: t.id }, { done: `${ticketNumber(t)} started.` });
        return;
      case "back_to_backlog":
        if (!effect.confirm) {
          moveLocally(t.id, to, null);
          void write("ticket.cancel", { ticket_id: t.id }, { done: `${ticketNumber(t)} is back in Backlog.` });
        } else setPending({ kind: "back_to_backlog", ticket: t });
        return;
      case "plan":
        open(t.id);
        return;
      default:
        setPending({ kind: effect.kind, ticket: t });
    }
  };

  // Moving through the cards goes by what is on screen: columns as they stand, without a second model.
  const focusCard = (step: "next" | "prev" | "right" | "left") => {
    const cards = [...document.querySelectorAll<HTMLElement>("[data-card]")];
    const here =
      document.activeElement instanceof HTMLElement && document.activeElement.dataset.card
        ? document.activeElement
        : (cards.find((c) => c.dataset.card === ticket) ?? null);
    if (!here) return cards[0]?.focus();
    if (step === "next" || step === "prev") {
      const same = cards.filter((c) => c.dataset.column === here.dataset.column);
      return same[same.indexOf(here) + (step === "next" ? 1 : -1)]?.focus();
    }
    const kinds = [...new Set(cards.map((c) => c.dataset.column))];
    const target = kinds[kinds.indexOf(here.dataset.column) + (step === "right" ? 1 : -1)];
    return cards.find((c) => c.dataset.column === target)?.focus();
  };
  useAction("SelectNext", () => focusCard("next"), dragging === null);
  useAction("SelectPrev", () => focusCard("prev"), dragging === null);
  useAction("NextTab", () => focusCard("right"), dragging === null);
  useAction("PrevTab", () => focusCard("left"), dragging === null);
  useAction("OpenSelected", () => {
    const id = document.activeElement instanceof HTMLElement ? document.activeElement.dataset.card : undefined;
    if (id) open(id);
  });

  const toggleColumn = (kind: ColumnKind) => {
    const next = hidden.includes(kind) ? hidden.filter((k) => k !== kind) : [...hidden, kind];
    setHidden(next);
    storage.set(HIDDEN_KEY, next.join(","));
  };
  const columnName = (kind: ColumnKind) => columns.find((c) => c.column.kind === kind)?.column.name ?? kind;
  const ownerName =
    owner.kind === "anyone"
      ? "Anyone"
      : owner.kind === "none"
        ? "No owner yet"
        : (employees.data?.find((e) => e.id === owner.id)?.name ?? "Someone");
  const projectName = projects.data?.find((p) => p.id === project)?.name ?? "All projects";
  const active = dragging ? all.find((t) => t.id === dragging) : undefined;
  const pill =
    "h-6 flex-none cursor-pointer rounded-[12px] border border-dashed whitespace-nowrap border-btn-line bg-transparent px-[9px] text-[12px] font-medium text-text2 hover:text-text";

  return (
    <>
      <header className="flex h-11 flex-none items-center gap-[10px] border-b border-line pr-3 pl-[18px]">
        <h1 className="m-0 text-[13px] font-medium">Board</h1>
        <Segments
          label="View"
          value={view}
          options={[
            { key: "board", label: "Board" },
            { key: "list", label: "List" },
          ]}
          onChange={setView}
        />
        {needsCount > 0 || needsOnly ? (
          <button
            type="button"
            aria-pressed={needsOnly}
            onClick={() => setNeedsOnly(!needsOnly)}
            className={`h-6 cursor-pointer rounded-[12px] border border-amber-line px-[9px] text-[12px] font-medium text-amber ${needsOnly ? "bg-amber-soft shadow-[inset_0_0_0_1px_var(--amber-line)]" : "bg-amber-soft"}`}
          >
            Needs you · {needsCount}
          </button>
        ) : null}
        {projectId ? null : (
          <Menu
            label="Project"
            value={project ?? "all"}
            options={[
              { key: "all", label: "All projects" },
              ...(projects.data ?? []).map((p) => ({ key: p.id, label: p.name })),
            ]}
            onPick={(id) => {
              const next = id === "all" ? null : id;
              setProject(next);
              storage.set("board_project", next ?? "");
            }}
            trigger={
              <button type="button" className={pill}>
                {projectName}
              </button>
            }
          />
        )}
        <Menu
          label="Owner"
          options={[
            { key: "anyone", label: "Anyone" },
            { key: "none", label: "No owner yet" },
            ...(employees.data ?? []).filter((e) => e.status !== "archived").map((e) => ({ key: e.id, label: e.name })),
          ]}
          onPick={(id) => setOwner(id === "anyone" || id === "none" ? { kind: id } : { kind: "employee", id })}
          trigger={
            <button type="button" className={pill}>
              {ownerName}
            </button>
          }
        />
        <input
          aria-label="Find a ticket"
          placeholder="Find"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="h-6 w-[130px] min-w-0 rounded-[12px] border border-line bg-transparent px-[10px] text-[12px] text-text outline-none placeholder:text-text3 focus:border-acc-text"
        />
        <div className="ml-auto flex flex-none gap-[6px]">
          <Menu
            label="Display"
            options={columns.map((c) => ({
              key: c.column.kind,
              label: `${hidden.includes(c.column.kind) ? "Show" : "Hide"} ${c.column.name}`,
            }))}
            onPick={toggleColumn}
            trigger={<Button kind="ghost">Display</Button>}
          />
          <Button kind="primary" onClick={() => overlays.open("ticket")}>
            New ticket <Kbd onAccent>N</Kbd>
          </Button>
        </div>
      </header>
      <div className="flex min-h-0 flex-1">
        {board.error ? (
          <p role="alert" className="m-0 flex-1 p-[18px] text-red">
            {board.error.message}
          </p>
        ) : view === "list" ? (
          <div className="flex min-w-0 flex-1 flex-col">
            <TicketRows
              tickets={shown.flatMap((c) => c.cards)}
              columnName={columnName}
              selected={ticket}
              needs={need.needs}
              mine={need.mine}
              onOpen={open}
            />
          </div>
        ) : (
          <DndContext
            sensors={sensors}
            collisionDetection={closestCorners}
            onDragStart={(e) => setDragging(String(e.active.id))}
            onDragEnd={drop}
            onDragCancel={() => setDragging(null)}
          >
            <div className="flex min-w-0 flex-1 items-start gap-3 overflow-x-auto bg-board py-3 pr-3 pl-[14px]">
              {shown
                .filter((c) => !hidden.includes(c.column.kind))
                .map((c) => (
                  <Column
                    key={c.column.id}
                    column={c.column}
                    cards={c.cards}
                    older={c.older}
                    total={c.cards.length + c.older}
                    need={need.needs}
                    mine={need.mine}
                    selected={ticket}
                    onOpen={open}
                    onShowOlder={() => setDoneAll(true)}
                  />
                ))}
              {hidden.length > 0 ? (
                <div className="flex w-[170px] flex-none flex-col gap-1 px-[6px]">
                  <div className="flex h-8 items-center font-medium text-text2">Hidden columns</div>
                  {shown
                    .filter((c) => hidden.includes(c.column.kind))
                    .map((c) => (
                      <button
                        key={c.column.id}
                        type="button"
                        onClick={() => toggleColumn(c.column.kind)}
                        className="flex h-[30px] cursor-pointer items-center gap-2 rounded-[6px] border border-line2 bg-sunk px-2 text-text2 hover:text-text"
                      >
                        <ColumnGlyph kind={c.column.kind} />
                        {c.column.name}
                        <span className="ml-auto text-text3">{c.cards.length + c.older}</span>
                      </button>
                    ))}
                </div>
              ) : null}
            </div>
            <DragOverlay dropAnimation={null}>
              {active ? <CardBody t={active} need={need.needs(active)} mine={need.mine(active)} selected /> : null}
            </DragOverlay>
          </DndContext>
        )}
        {ticket ? <TicketPanel key={ticket} id={ticket} onClose={close} /> : null}
      </div>
      <Dialog
        open={pending?.kind === "send_back"}
        onClose={() => setPending(null)}
        title="Send back to Doing"
        width={480}
      >
        {pending?.kind === "send_back" ? <SendBackDialog t={pending.ticket} onClose={() => setPending(null)} /> : null}
      </Dialog>
      {pending && pending.kind !== "send_back" ? (
        <ConfirmDialog
          open
          onClose={() => setPending(null)}
          title={
            pending.kind === "mark_done"
              ? `Mark ${ticketNumber(pending.ticket)} as Done?`
              : pending.kind === "reject"
                ? `Reject the plan of ${ticketNumber(pending.ticket)}?`
                : `Back to Backlog: ${ticketNumber(pending.ticket)}?`
          }
          body={
            pending.kind === "mark_done"
              ? "This closes the ticket. Nothing is merged, pushed or deployed."
              : pending.kind === "reject"
                ? "The ticket goes back to Backlog."
                : "This stops the work."
          }
          confirm={
            pending.kind === "mark_done" ? "Mark done" : pending.kind === "reject" ? "Afwijzen" : "Back to Backlog"
          }
          danger={pending.kind === "reject"}
          onConfirm={() =>
            write(pending.kind === "mark_done" ? "ticket.accept" : "ticket.cancel", { ticket_id: pending.ticket.id })
          }
        />
      ) : null}
    </>
  );
}
