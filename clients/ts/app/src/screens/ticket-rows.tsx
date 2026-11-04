import type { ColumnKind, TicketView } from "@aurelius/protocol";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef } from "react";
import type { Needs } from "./board-logic";
import { ColumnGlyph, NeedsTag, OwnerMark, TicketTags, ticketNumber } from "./tickets-kit";

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
    <div ref={scroller} className="min-h-0 flex-1 overflow-auto">
      <div className="relative min-w-[640px]" style={{ height: virtual.getTotalSize() }}>
        {virtual.getVirtualItems().map((item) => {
          const row = rows[item.index] as Row;
          const place = { height: item.size, transform: `translateY(${item.start}px)` };
          if (!row.t) {
            return (
              <div
                key={row.kind}
                style={place}
                className="absolute inset-x-0 flex items-center gap-2 border-b border-line2 bg-sunk px-[18px] font-medium"
              >
                <ColumnGlyph kind={row.kind} />
                {columnName(row.kind)}
                <span className="font-normal text-text3">{row.count}</span>
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
              style={place}
              className={`absolute inset-x-0 grid w-full cursor-pointer grid-cols-[22px_56px_minmax(0,1fr)_auto_100px_44px] items-center gap-3 border-0 border-b border-solid border-line2 px-[18px] text-left text-text hover:bg-sel ${t.id === selected ? "bg-sel" : "bg-transparent"}`}
            >
              <ColumnGlyph kind={row.kind} />
              <span className="font-mono text-[12px] text-text3">{ticketNumber(t)}</span>
              <span className="flex min-w-0 items-center gap-[10px]">
                <span className="truncate font-medium">{t.title}</span>
                {need ? <NeedsTag needs={need} mine={mine(t)} /> : null}
              </span>
              <span className="flex gap-[6px]">
                <TicketTags t={t} />
              </span>
              <span className="flex min-w-0 items-center gap-[6px] text-text2">
                <OwnerMark t={t} />
                <span className="truncate">{t.owner_name ?? ""}</span>
              </span>
              <span className="text-right text-[12px] text-text3">{ago(t.updated_at_ms)}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
