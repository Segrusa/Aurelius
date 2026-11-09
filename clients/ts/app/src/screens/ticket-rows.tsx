import type { ColumnKind, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef } from "react";
import type { Needs } from "./board-logic";
import { ColumnGlyph, NeedsTag, OwnerMark, TicketTags, ticketNumber } from "./tickets-kit";

const styles = stylex.create({
  scroller: { minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", overflow: "auto" },
  track: { position: "relative", minWidth: 640 },
  head: {
    position: "absolute",
    insetInline: 0,
    display: "flex",
    alignItems: "center",
    gap: 8,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 18,
    fontWeight: 500,
  },
  count: { fontWeight: 400, color: "var(--precision-text3)" },
  row: {
    position: "absolute",
    insetInline: 0,
    display: "grid",
    width: "100%",
    cursor: "pointer",
    gridTemplateColumns: "22px 56px minmax(0,1fr) auto 100px 44px",
    alignItems: "center",
    gap: 12,
    borderWidth: 0,
    borderBottomWidth: 1,
    borderStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 18,
    textAlign: "left",
    color: "var(--color-text-primary)",
  },
  selected: {
    backgroundColor: "var(--color-overlay-hover)",
    boxShadow: "inset 2px 0 0 var(--color-text-accent)",
  },
  number: { fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--precision-text3)" },
  titleCell: { display: "flex", minWidth: 0, alignItems: "center", gap: 10 },
  title: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", fontWeight: 500 },
  tags: { display: "flex", gap: 6 },
  owner: {
    display: "flex",
    minWidth: 0,
    alignItems: "center",
    gap: 6,
    color: "var(--color-text-secondary)",
  },
  ownerName: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  age: { textAlign: "right", fontSize: 12, color: "var(--precision-text3)" },
});

const ROW_HEIGHT = 44;
const HEAD_HEIGHT = 34;

interface Row {
  kind: ColumnKind;
  t?: TicketView;
  count?: number;
}

const ORDER: ColumnKind[] = ["review", "plan_review", "testing", "doing", "todo", "backlog", "done"];

export function ago(ms: number): string {
  const minutes = Math.max(0, Math.round((Date.now() - ms) / 60_000));
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  return hours < 48 ? `${hours}h` : `${Math.round(hours / 24)}d`;
}

/** Tickets grouped by column, the most urgent column first. */
export function TicketRows({
  tickets,
  columnName,
  selected,
  needs,
  mine,
  onOpen,
}: {
  tickets: TicketView[];
  columnName: (kind: ColumnKind) => string;
  selected: string | undefined;
  needs: (t: TicketView) => Needs | null;
  mine: (t: TicketView) => boolean;
  onOpen: (id: string) => void;
}) {
  const rows = useMemo(
    () =>
      ORDER.flatMap((kind): Row[] => {
        const list = tickets.filter((t) => t.column === kind);
        return list.length === 0 ? [] : [{ kind, count: list.length }, ...list.map((t) => ({ kind, t }))];
      }),
    [tickets],
  );
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => (rows[i]?.t ? ROW_HEIGHT : HEAD_HEIGHT),
    overscan: 10,
  });
  const at = rows.findIndex((r) => r.t?.id === selected);
  useEffect(() => {
    if (at >= 0) virtual.scrollToIndex(at, { align: "auto" });
  }, [at, virtual]);
  return (
    <div ref={scroller} {...stylex.props(styles.scroller)}>
      <div {...stylex.props(styles.track)} style={{ height: virtual.getTotalSize() }}>
        {virtual.getVirtualItems().map((item) => {
          const row = rows[item.index] as Row;
          const place = { height: item.size, transform: `translateY(${item.start}px)` };
          if (!row.t) {
            return (
              <div key={row.kind} {...stylex.props(styles.head)} style={place}>
                <ColumnGlyph kind={row.kind} />
                {columnName(row.kind)}
                <span {...stylex.props(styles.count)}>{row.count}</span>
              </div>
            );
          }
          const t = row.t;
          const need = needs(t);
          return (
            <button
              key={t.id}
              type="button"
              data-ticket={t.id}
              aria-current={t.id === selected ? "true" : undefined}
              onClick={() => onOpen(t.id)}
              {...stylex.props(styles.row, t.id === selected && styles.selected)}
              style={place}
            >
              <ColumnGlyph kind={row.kind} />
              <span {...stylex.props(styles.number)}>{ticketNumber(t)}</span>
              <span {...stylex.props(styles.titleCell)}>
                <span {...stylex.props(styles.title)}>{t.title}</span>
                {need ? <NeedsTag needs={need} mine={mine(t)} /> : null}
              </span>
              <span {...stylex.props(styles.tags)}>
                <TicketTags t={t} />
              </span>
              <span {...stylex.props(styles.owner)}>
                <OwnerMark t={t} />
                <span {...stylex.props(styles.ownerName)}>{t.owner_name ?? ""}</span>
              </span>
              <span {...stylex.props(styles.age)}>{ago(t.updated_at_ms)}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
