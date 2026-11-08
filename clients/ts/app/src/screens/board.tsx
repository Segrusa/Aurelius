import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Card } from "@astryxdesign/core/Card";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Layout, LayoutContent } from "@astryxdesign/core/Layout";
import { Link } from "@astryxdesign/core/Link";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
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
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useBoard, useCompany, useEmployees, useMembers, useProjects } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { NewIcon } from "../ui/icons";
import { KeyHint } from "../ui/key-hint";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import {
  beforeFor,
  columnShown,
  columnsProblem,
  dropEffect,
  emptyText,
  movedCard,
  type Needs,
  type OwnerFilter,
  type ResponsibleFilter,
  shownCards,
} from "./board-logic";
import { Held, useSend } from "./dialogs";
import { ticketLink } from "./links-logic";
import { FormDialog, Page, PageHead, Problem } from "./org-kit";
import { overlays } from "./overlays-store";
import { ConfirmDialog, TicketPanel } from "./ticket-panel";
import { TicketRows } from "./ticket-rows";
import {
  ColumnGlyph,
  NeedsTag,
  NoConnectionNote,
  OwnerMark,
  TicketTags,
  ticketNumber,
  useNoConnection,
  useTicketNeeds,
  useTicketWrite,
} from "./tickets-kit";

const styles = stylex.create({
  columnsRow: { display: "flex", alignItems: "center", gap: 8 },
  card: {
    display: "flex",
    width: "100%",
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 8,
    borderRadius: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    backgroundColor: "var(--color-background-card)",
    paddingInline: 12,
    paddingBlock: 10,
    textAlign: "left",
    color: "var(--color-text-primary)",
    boxShadow: "var(--precision-card-shadow)",
  },
  cardSelected: {
    display: "flex",
    width: "100%",
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 8,
    borderRadius: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-text-accent)",
    backgroundColor: "var(--color-background-card)",
    paddingInline: 12,
    paddingBlock: 10,
    textAlign: "left",
    color: "var(--color-text-primary)",
    boxShadow: "0 0 0 1px var(--color-text-accent)",
  },
  cardHead: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  cardOwner: { marginInlineStart: "auto" },
  cardTitle: { lineHeight: 1.4, fontWeight: 500 },
  cardTags: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 6, ":empty": { display: "none" } },
  cardButton: {
    display: "block",
    width: "100%",
    cursor: "pointer",
    borderRadius: 7,
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    textAlign: "left",
  },
  column: {
    display: "flex",
    maxHeight: "100%",
    width: 248,
    flexShrink: 0,
    flexDirection: "column",
    gap: 6,
    borderRadius: 8,
  },
  columnFolded: {
    display: "flex",
    maxHeight: "100%",
    width: 36,
    flexShrink: 0,
    flexDirection: "column",
    alignItems: "center",
    gap: 8,
    borderRadius: 8,
    paddingBlock: 8,
  },
  over: { backgroundColor: "var(--color-overlay-hover)" },
  foldedName: { fontWeight: 500, color: "var(--precision-text3)", writingMode: "vertical-rl" },
  columnHead: { display: "flex", height: 32, flexShrink: 0, alignItems: "center", gap: 8, paddingInline: 6 },
  columnName: { fontWeight: 500 },
  columnCount: { color: "var(--precision-text3)" },
  columnAdd: { marginInlineStart: "auto" },
  columnScroll: { minHeight: 40, overflowY: "auto", paddingBottom: 4 },
  columnCards: { display: "flex", flexDirection: "column", gap: 6 },
  columnEmpty: { margin: 0, paddingInline: 6, paddingBlock: 4, fontSize: 12, color: "var(--precision-text3)" },
  showOlder: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 6,
    textAlign: "left",
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  needs: {
    height: 24,
    flexShrink: 0,
    cursor: "pointer",
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-warning)",
    backgroundColor: "var(--color-warning-muted)",
    paddingInline: 9,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-warning)",
  },
  needsOn: { boxShadow: "inset 0 0 0 1px var(--color-border-warning)" },
  pill: {
    height: 24,
    flexShrink: 0,
    cursor: "pointer",
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "dashed",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "transparent",
    paddingInline: 9,
    fontSize: 12,
    fontWeight: 500,
    whiteSpace: "nowrap",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  body: { position: "relative", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  fill: { position: "absolute", inset: 0 },
  message: { padding: 18 },
  main: { display: "flex", height: "100%", minWidth: 0, flexDirection: "column" },
  columns: {
    display: "flex",
    height: "100%",
    minWidth: 0,
    boxSizing: "border-box",
    alignItems: "flex-start",
    gap: 12,
    overflowX: "auto",
    backgroundColor: "var(--color-background-muted)",
    paddingBlock: 12,
    paddingInlineStart: 14,
    paddingInlineEnd: 12,
  },
  hidden: { display: "flex", width: 170, flexShrink: 0, flexDirection: "column", gap: 4, paddingInline: 6 },
  hiddenHead: {
    display: "flex",
    height: 32,
    alignItems: "center",
    fontWeight: 500,
    color: "var(--color-text-secondary)",
  },
  hiddenColumn: {
    display: "flex",
    height: 30,
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--precision-line2)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 8,
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  hiddenCount: { marginInlineStart: "auto", color: "var(--precision-text3)" },
  menuAnchor: { position: "fixed" },
});

const HIDDEN_KEY = "board_hidden_columns";
const CARD_HEIGHT = 88;

function ColumnsDialog({ columns, onClose }: { columns: BoardColumnView[]; onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [names, setNames] = useState(() => new Map(columns.map((c) => [c.id, c.name])));
  const [ordered, setOrdered] = useState(columns);
  const shift = (index: number, by: number) => {
    const next = [...ordered];
    const [moved] = next.splice(index, 1);
    if (!moved) return;
    next.splice(index + by, 0, moved);
    setOrdered(next);
  };
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const save = async () => {
    const edits = ordered.map((c) => ({ column_id: c.id, name: (names.get(c.id) ?? "").trim() }));
    const wrong = columnsProblem(edits.map((e, i) => ({ name: e.name, title: columns[i]?.name ?? "" })));
    setProblem(wrong);
    if (wrong) return;
    setBusy(true);
    try {
      await connection.write(
        "board.columns_update",
        { columns: edits },
        { retry: false, expectedVersion: Math.max(...columns.map((c) => c.version)) },
      );
      await client.invalidateQueries({ queryKey: ["board"] });
      toast("Columns saved. Every client shows them now.");
      onClose();
    } catch (e) {
      setProblem(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <FormDialog
      title="Columns"
      width={420}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Save columns" isDisabled={busy} onClick={() => void save()} />
        </>
      }
    >
      {ordered.map((c, i) => (
        <div key={c.id} {...stylex.props(styles.columnsRow)}>
          <IconButton
            variant="ghost"
            size="sm"
            label={`Move ${c.name} left`}
            icon={<Icon icon="chevronLeft" />}
            isDisabled={i === 0}
            onClick={() => shift(i, -1)}
          />
          <IconButton
            variant="ghost"
            size="sm"
            label={`Move ${c.name} right`}
            icon={<Icon icon="chevronRight" />}
            isDisabled={i === ordered.length - 1}
            onClick={() => shift(i, 1)}
          />
          <ColumnGlyph kind={c.kind} />
          <TextInput
            label={`Name of ${c.name}`}
            isLabelHidden
            size="sm"
            value={names.get(c.id) ?? ""}
            onChange={(value) => setNames(new Map(names).set(c.id, value))}
          />
        </div>
      ))}
      {problem ? <Problem>{problem}</Problem> : null}
    </FormDialog>
  );
}

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
    <Card xstyle={selected ? styles.cardSelected : styles.card}>
      <span {...stylex.props(styles.cardHead)}>
        {ticketNumber(t)}
        <span {...stylex.props(styles.cardOwner)}>
          <OwnerMark t={t} />
        </span>
      </span>
      <span {...stylex.props(styles.cardTitle)}>{t.title}</span>
      <span {...stylex.props(styles.cardTags)}>
        {need ? <NeedsTag needs={need} mine={mine} /> : null}
        <TicketTags t={t} />
      </span>
    </Card>
  );
});

function TicketCard({
  t,
  kind,
  need,
  mine,
  selected,
  onOpen,
  onMenu,
}: {
  t: TicketView;
  kind: ColumnKind;
  need: Needs | null;
  mine: boolean;
  selected: boolean;
  onOpen: (id: string) => void;
  onMenu: (t: TicketView, x: number, y: number) => void;
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
      onContextMenu={(e) => {
        e.preventDefault();
        const { clientX: x, clientY: y } = e;
        // Where the button is still down (Linux, macOS), a popover shown now is light-dismissed by its release.
        if (e.buttons === 0) onMenu(t, x, y);
        else window.addEventListener("pointerup", () => setTimeout(() => onMenu(t, x, y)), { once: true });
      }}
      {...attributes}
      {...listeners}
      style={{ transform: CSS.Translate.toString(transform), transition, opacity: isDragging ? 0.4 : 1 }}
      {...stylex.props(styles.cardButton)}
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
  onMenu,
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
  onMenu: (t: TicketView, x: number, y: number) => void;
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
  // Only Plan mode uses this column: while it is empty it gives its room to the others.
  if (column.kind === "plan_review" && total === 0) {
    return (
      <section
        aria-label={column.name}
        ref={setNodeRef}
        title={emptyText(column.kind)}
        {...stylex.props(styles.columnFolded, isOver && styles.over)}
      >
        <ColumnGlyph kind={column.kind} />
        <span {...stylex.props(styles.foldedName)}>{column.name}</span>
      </section>
    );
  }
  return (
    <section aria-label={column.name} ref={setNodeRef} {...stylex.props(styles.column, isOver && styles.over)}>
      <div {...stylex.props(styles.columnHead)}>
        <ColumnGlyph kind={column.kind} />
        <span {...stylex.props(styles.columnName)}>{column.name}</span>
        <span {...stylex.props(styles.columnCount)}>{total}</span>
        {column.kind === "backlog" ? (
          <span {...stylex.props(styles.columnAdd)}>
            <IconButton
              variant="ghost"
              size="sm"
              label={`Add ticket to ${column.name}`}
              icon={<NewIcon />}
              onClick={() => overlays.open("ticket")}
            />
          </span>
        ) : null}
      </div>
      <div ref={scroller} data-column-scroll {...stylex.props(styles.columnScroll)}>
        <SortableContext items={ids} strategy={verticalListSortingStrategy}>
          {/* In flow between two paddings: the sortable transforms need the cards where they stand. */}
          <div style={{ paddingTop: top, paddingBottom: bottom }} {...stylex.props(styles.columnCards)}>
            {items.map((item) => {
              const t = cards[item.index] as TicketView;
              return (
                <div key={t.id} data-index={item.index} ref={virtual.measureElement}>
                  <TicketCard
                    t={t}
                    kind={column.kind}
                    need={need(t)}
                    mine={mine(t)}
                    selected={t.id === selected}
                    onOpen={onOpen}
                    onMenu={onMenu}
                  />
                </div>
              );
            })}
          </div>
        </SortableContext>
        {cards.length === 0 ? <p {...stylex.props(styles.columnEmpty)}>{emptyText(column.kind)}</p> : null}
        {older > 0 ? (
          <button type="button" onClick={onShowOlder} {...stylex.props(styles.showOlder)}>
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
    <FormDialog
      title={`Send ${ticketNumber(t)} back to Doing`}
      width={480}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Send back" isDisabled={busy || feedback.trim() === ""} onClick={go} />
        </>
      }
    >
      <TextArea
        hasAutoFocus
        label="What needs to change?"
        isLabelHidden
        placeholder="What needs to change?"
        rows={4}
        value={feedback}
        onChange={setFeedback}
      />
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem>{error}</Problem> : null}
    </FormDialog>
  );
}

type Pending = { kind: "send_back" | "mark_done" | "back_to_backlog" | "reject"; ticket: TicketView } | null;

export function BoardScreen({
  ticket,
  projectId,
  message,
}: {
  ticket: string | undefined;
  message?: string;
  projectId?: string;
}) {
  useKeyContext("Board");
  const navigate = useNavigate();
  const client = useQueryClient();
  const { storage } = usePlatform();
  const { features, user, hostId } = useLinkState();
  const me = user?.id ?? null;
  const write = useTicketWrite();
  const [chosenProject, setProject] = useState<string | null>(() => storage.get("board_project"));
  const project = projectId ?? chosenProject;
  const [owner, setOwner] = useState<OwnerFilter>({ kind: "anyone" });
  const [responsible, setResponsible] = useState<ResponsibleFilter>({ kind: "anyone" });
  const members = useMembers();
  const [query, setQuery] = useState("");
  const [needsOnly, setNeedsOnly] = useState(false);
  const [doneAll, setDoneAll] = useState(false);
  const [view, setView] = useState<"board" | "list">("board");
  const [hidden, setHidden] = useState<ColumnKind[]>(
    () => (storage.get(HIDDEN_KEY)?.split(",").filter(Boolean) ?? []) as ColumnKind[],
  );
  const [dragging, setDragging] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<TicketView | null>(null);
  const platform = usePlatform();
  const [pending, setPending] = useState<Pending>(null);
  const projects = useProjects();
  const employees = useEmployees();
  const board = useBoard(project);
  const [renaming, setRenaming] = useState(false);
  const { claudeLogin } = useLinkState();
  const all = useMemo(() => (board.data?.columns ?? []).flatMap((c) => c.tickets), [board.data]);
  const need = useTicketNeeds(all);
  const needsCount = useMemo(() => all.filter(need.mine).length, [all, need]);
  const company = useCompany();
  const noConnection = useNoConnection();
  // Testing only where someone tests, or a card is still there.
  const tester = Boolean(company.data?.company.tester_employee_id);
  const testing = features.includes("board_testing");
  const columns = useMemo(
    () =>
      (board.data?.columns ?? []).filter((c) =>
        columnShown(c.column.kind, testing && (tester || c.tickets.length > 0)),
      ),
    [board.data, testing, tester],
  );
  // The panel takes room from the board: the opened card stays in view.
  useEffect(() => {
    if (!ticket) return;
    const frame = requestAnimationFrame(() =>
      document.querySelector(`[data-card="${ticket}"]`)?.scrollIntoView({ inline: "nearest", block: "nearest" }),
    );
    return () => cancelAnimationFrame(frame);
  }, [ticket]);
  const filter = useMemo(
    () => ({ owner, responsible, query, needsOnly, needs: need.mine }),
    [owner, responsible, query, needsOnly, need],
  );
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
    moveCard(t, to, overId.startsWith("column:") ? null : overId);
  };

  /** A drag or the Move menu: the same rules. */
  const moveCard = (t: TicketView, to: ColumnKind, onto: string | null) => {
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

  // One menu for right click and keyboard: it stays mounted, because a DropdownMenu mounted already open
  // takes no focus, and what it last showed stays while it animates away.
  const [cardMenu, setCardMenu] = useState<{ t: TicketView; x: number; y: number } | null>(null);
  const [cardMenuOpen, setCardMenuOpen] = useState(false);
  const openMenu = useCallback((t: TicketView, x: number, y: number) => {
    setCardMenu({ t, x, y });
    setCardMenuOpen(true);
  }, []);
  const copyLink = (t: TicketView) =>
    void platform.copy(ticketLink(hostId ?? "", t.id)).then(() => toast("Link copied."));
  const pickFromMenu = (t: TicketView, key: string) => {
    if (key === "open") open(t.id);
    else if (key === "start") void write("ticket.start", { ticket_id: t.id }, { done: `${ticketNumber(t)} started.` });
    else if (key.startsWith("start:"))
      void write(
        "ticket.start",
        { ticket_id: t.id, employee_id: key.slice("start:".length) },
        { done: `${ticketNumber(t)} started.` },
      );
    else if (key === "copy") copyLink(t);
    else if (key === "follow" || key === "unfollow")
      void write(
        "ticket.follow",
        { ticket_id: t.id, on: key === "follow" },
        { done: key === "follow" ? "You follow this ticket." : "You don't follow this ticket any more." },
      );
    else if (key.startsWith("responsible:")) {
      const who = key.slice("responsible:".length);
      void write("ticket.set_responsible", { ticket_id: t.id, responsible_user_id: who === "none" ? null : who });
    } else if (key.startsWith("move:")) moveCard(t, key.slice("move:".length) as ColumnKind, null);
    else if (key === "delete") setDeleting(t);
  };
  const cardOptions = (t: TicketView) => [
    { key: "open", label: "Open" },
    ...(t.column === "backlog"
      ? [
          { key: "start", label: "Start" },
          ...(employees.data ?? [])
            .filter((e) => e.status === "active")
            .map((e) => ({ key: `start:${e.id}`, label: `Start with ${e.name}` })),
        ]
      : []),
    { key: "copy", label: "Copy link" },
    ...(features.includes("ticket_follow")
      ? [
          { key: "follow", label: "Follow" },
          { key: "unfollow", label: "Unfollow" },
        ]
      : []),
    ...(features.includes("ticket_responsible") && t.column !== "done"
      ? [
          ...(me && t.responsible_user_id !== me ? [{ key: `responsible:${me}`, label: "Make me responsible" }] : []),
          ...(me && t.responsible_user_id === me
            ? [{ key: "responsible:none", label: "Remove me as responsible" }]
            : []),
          ...(members.data ?? [])
            .filter((u) => !u.disabled_at_ms && u.id !== me && u.id !== t.responsible_user_id)
            .map((u) => ({ key: `responsible:${u.id}`, label: `Responsible: ${u.username}` })),
        ]
      : []),
    ...columns
      .filter((c) => c.column.kind !== t.column)
      .map((c) => ({ key: `move:${c.column.kind}`, label: `Move to ${c.column.name}` })),
    ...(t.column === "backlog" ? [{ key: "delete", label: "Delete…", danger: true }] : []),
  ];
  /** The card with the keyboard's focus. */
  const focusedCard = () => {
    const el = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const t = all.find((x) => x.id === el?.dataset.card);
    return t && el ? { t, el } : null;
  };
  const menuOnCard = () => {
    const card = focusedCard();
    if (!card) return;
    const box = card.el.getBoundingClientRect();
    openMenu(card.t, box.left, box.bottom);
  };
  useAction("BoardMoveTo", menuOnCard);
  useAction("BoardResponsible", menuOnCard);
  useAction("BoardStart", () => {
    const card = focusedCard();
    if (card?.t.column === "backlog") moveCard(card.t, "doing", null);
  });
  useAction("BoardFollow", () => {
    const card = focusedCard();
    if (card) pickFromMenu(card.t, "follow");
  });
  useAction("BoardCopyLink", () => {
    const card = focusedCard();
    if (card) copyLink(card.t);
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

  return (
    <Page>
      <PageHead
        title="Board"
        tabs={
          <>
            <SegmentedControl
              label="View"
              size="sm"
              value={view}
              onChange={(next) => setView(next as "board" | "list")}
            >
              <SegmentedControlItem value="board" label="Board" />
              <SegmentedControlItem value="list" label="List" />
            </SegmentedControl>
            {needsCount > 0 || needsOnly ? (
              <button
                type="button"
                aria-pressed={needsOnly}
                onClick={() => setNeedsOnly(!needsOnly)}
                {...stylex.props(styles.needs, needsOnly && styles.needsOn)}
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
                  <button type="button" {...stylex.props(styles.pill)}>
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
                ...(employees.data ?? [])
                  .filter((e) => e.status !== "archived")
                  .map((e) => ({ key: e.id, label: e.name })),
              ]}
              onPick={(id) => setOwner(id === "anyone" || id === "none" ? { kind: id } : { kind: "employee", id })}
              trigger={
                <button type="button" {...stylex.props(styles.pill)}>
                  {ownerName}
                </button>
              }
            />
            {features.includes("ticket_responsible") ? (
              <Menu
                label="Responsible"
                options={[
                  { key: "anyone", label: "Anyone" },
                  ...(me ? [{ key: me, label: "Me" }] : []),
                  { key: "none", label: "No one" },
                  ...(members.data ?? [])
                    .filter(
                      (u) => u.id !== me && (!u.disabled_at_ms || all.some((t) => t.responsible_user_id === u.id)),
                    )
                    .map((u) => ({ key: u.id, label: u.username })),
                ]}
                onPick={(id) => setResponsible(id === "anyone" || id === "none" ? { kind: id } : { kind: "user", id })}
                trigger={
                  <button type="button" {...stylex.props(styles.pill)}>
                    {responsible.kind === "anyone"
                      ? "Responsible: anyone"
                      : responsible.kind === "none"
                        ? "Responsible: no one"
                        : responsible.id === me
                          ? "Responsible: me"
                          : `Responsible: ${members.data?.find((u) => u.id === responsible.id)?.username ?? "…"}`}
                  </button>
                }
              />
            ) : null}
            <TextInput
              label="Find a ticket"
              isLabelHidden
              placeholder="Find"
              size="sm"
              width={130}
              value={query}
              onChange={setQuery}
            />
          </>
        }
      >
        <Button variant="ghost" label="Columns" onClick={() => setRenaming(true)} />
        <Menu
          label="Display"
          options={columns.map((c) => ({
            key: c.column.kind,
            label: `${hidden.includes(c.column.kind) ? "Show" : "Hide"} ${c.column.name}`,
          }))}
          onPick={toggleColumn}
          trigger={<Button variant="ghost" label="Display" />}
        />
        <Button variant="primary" label="New ticket" onClick={() => overlays.open("ticket")}>
          New ticket <KeyHint onAccent>N</KeyHint>
        </Button>
      </PageHead>
      {noConnection ? <Banner status="warning" container="section" title={<NoConnectionNote />} /> : null}
      {claudeLogin && claudeLogin.status !== "linked" ? (
        <Banner
          status="warning"
          container="section"
          title={
            claudeLogin.status === "expired"
              ? "Your Claude login expired. Reconnect it to run your tickets."
              : "Connect your Claude login to run your tickets."
          }
          endContent={
            <Link href="/settings/account">{claudeLogin.status === "expired" ? "Reconnect" : "Connect"}</Link>
          }
        />
      ) : null}
      {renaming && board.data ? (
        <ColumnsDialog columns={board.data.columns.map((c) => c.column)} onClose={() => setRenaming(false)} />
      ) : null}
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.fill)}>
          <Layout
            height="fill"
            end={
              ticket ? <TicketPanel key={ticket} id={ticket} onClose={close} {...(message ? { message } : {})} /> : null
            }
            content={
              <LayoutContent isScrollable={false} padding={0}>
                {board.error ? (
                  <div {...stylex.props(styles.message)}>
                    <Problem>{board.error.message}</Problem>
                  </div>
                ) : view === "list" ? (
                  <div {...stylex.props(styles.main)}>
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
                    <div {...stylex.props(styles.columns)}>
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
                            onMenu={openMenu}
                            onShowOlder={() => setDoneAll(true)}
                          />
                        ))}
                      {hidden.length > 0 ? (
                        <div {...stylex.props(styles.hidden)}>
                          <div {...stylex.props(styles.hiddenHead)}>Hidden columns</div>
                          {shown
                            .filter((c) => hidden.includes(c.column.kind))
                            .map((c) => (
                              <button
                                key={c.column.id}
                                type="button"
                                onClick={() => toggleColumn(c.column.kind)}
                                {...stylex.props(styles.hiddenColumn)}
                              >
                                <ColumnGlyph kind={c.column.kind} />
                                {c.column.name}
                                <span {...stylex.props(styles.hiddenCount)}>{c.cards.length + c.older}</span>
                              </button>
                            ))}
                        </div>
                      ) : null}
                    </div>
                    <DragOverlay dropAnimation={null}>
                      {active ? (
                        <CardBody t={active} need={need.needs(active)} mine={need.mine(active)} selected />
                      ) : null}
                    </DragOverlay>
                  </DndContext>
                )}
              </LayoutContent>
            }
          />
        </div>
      </div>
      {pending?.kind === "send_back" ? <SendBackDialog t={pending.ticket} onClose={() => setPending(null)} /> : null}
      <Menu
        open={cardMenuOpen}
        label={cardMenu ? `${ticketNumber(cardMenu.t)} actions` : "Ticket actions"}
        onOpenChange={setCardMenuOpen}
        options={cardMenu ? cardOptions(cardMenu.t) : []}
        onPick={(key) => {
          if (cardMenu) pickFromMenu(cardMenu.t, key);
        }}
        trigger={
          <span
            aria-hidden="true"
            {...stylex.props(styles.menuAnchor)}
            style={{ left: cardMenu?.x ?? 0, top: cardMenu?.y ?? 0 }}
          />
        }
      />
      {deleting ? (
        <ConfirmDialog
          open
          danger
          title={`Delete ${ticketNumber(deleting)}?`}
          body="It leaves the board and stays readable for the record."
          confirm="Delete"
          onClose={() => setDeleting(null)}
          onConfirm={() => {
            void write("ticket.archive", { ticket_id: deleting.id }, { done: `${ticketNumber(deleting)} is deleted.` });
            setDeleting(null);
          }}
        />
      ) : null}
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
            pending.kind === "mark_done" ? "Mark done" : pending.kind === "reject" ? "Reject" : "Back to Backlog"
          }
          danger={pending.kind === "reject"}
          onConfirm={() =>
            write(pending.kind === "mark_done" ? "ticket.accept" : "ticket.cancel", { ticket_id: pending.ticket.id })
          }
        />
      ) : null}
    </Page>
  );
}
