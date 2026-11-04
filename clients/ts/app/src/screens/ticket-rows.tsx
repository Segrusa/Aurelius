import type { ColumnKind, TicketView } from "@aurelius/protocol";
import type { Needs } from "./board-logic";
import { ColumnGlyph, NeedsTag, OwnerMark, TicketTags, ticketNumber } from "./tickets-kit";

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
  return (
    <div className="min-w-[640px]">
      {ORDER.map((kind) => {
        const rows = tickets.filter((t) => t.column === kind);
        if (rows.length === 0) return null;
        return (
          <section key={kind} aria-label={columnName(kind)}>
            <div className="flex h-[34px] items-center gap-2 border-b border-line2 bg-sunk px-[18px] font-medium">
              <ColumnGlyph kind={kind} />
              {columnName(kind)}
              <span className="font-normal text-text3">{rows.length}</span>
            </div>
            {rows.map((t) => {
              const need = needs(t);
              return (
                <button
                  key={t.id}
                  type="button"
                  data-ticket={t.id}
                  aria-current={t.id === selected ? "true" : undefined}
                  onClick={() => onOpen(t.id)}
                  className={`grid h-11 w-full cursor-pointer grid-cols-[22px_56px_minmax(0,1fr)_auto_100px_44px] items-center gap-3 border-0 border-b border-solid border-line2 px-[18px] text-left text-text hover:bg-sel ${t.id === selected ? "bg-sel" : "bg-transparent"}`}
                >
                  <ColumnGlyph kind={kind} />
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
          </section>
        );
      })}
    </div>
  );
}
