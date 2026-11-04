import type { ColumnKind } from "@aurelius/protocol";
import { useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useLinkState } from "../connection";
import { useBoard, useMyTickets } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { Button, Kbd } from "../ui/kit";
import { inMyTab, MY_TABS, type MyTab } from "./board-logic";
import { overlays } from "./overlays-store";
import { TicketPanel } from "./ticket-panel";
import { TicketRows } from "./ticket-rows";
import { useTicketNeeds } from "./tickets-kit";

export function MyTicketsScreen({ ticket }: { ticket: string | undefined }) {
  useKeyContext("MyTickets");
  const navigate = useNavigate();
  const { features, user } = useLinkState();
  const supported = features.includes("my_tickets");
  const [includeDone, setIncludeDone] = useState(false);
  const [chosen, setChosen] = useState<MyTab | null>(null);
  const mine = useMyTickets(null, includeDone, supported);
  const board = useBoard(null);
  const tickets = useMemo(() => mine.data?.tickets ?? [], [mine.data]);
  const need = useTicketNeeds(tickets);
  const me = user?.id ?? null;
  const counts = useMemo(
    () =>
      Object.fromEntries(
        MY_TABS.map((tab) => [
          tab.key,
          tickets.filter((t) => inMyTab(tab.key, t, mine.data?.following ?? [], me, need.mine(t))).length,
        ]),
      ) as Record<MyTab, number>,
    [tickets, mine.data, me, need],
  );
  // The first tab: Waiting on me when something waits.
  const tab: MyTab = chosen ?? (counts.waiting > 0 ? "waiting" : "responsible");
  const shown = useMemo(
    () => tickets.filter((t) => inMyTab(tab, t, mine.data?.following ?? [], me, need.mine(t))),
    [tickets, tab, mine.data, me, need],
  );
  const open = (id: string) => void navigate({ to: "/my-tickets", search: { ticket: id } });
  const close = () => void navigate({ to: "/my-tickets", search: {} });

  const at = shown.findIndex((t) => t.id === ticket);
  const step = (by: number) => {
    const next = shown[at < 0 ? 0 : at + by];
    if (next) open(next.id);
  };
  useAction("SelectNext", () => step(1));
  useAction("SelectPrev", () => step(-1));
  MY_TABS.forEach(({ key }, i) => {
    // biome-ignore lint/correctness/useHookAtTopLevel: a fixed list: the same hooks in the same order on every render.
    useAction(`Filter${i + 1}`, () => setChosen(key));
  });
  const columnName = (kind: ColumnKind) => board.data?.columns.find((c) => c.column.kind === kind)?.column.name ?? kind;
  const empty = MY_TABS.find((t) => t.key === tab)?.empty;

  return (
    <>
      <header className="flex h-11 flex-none items-center gap-[10px] border-b border-line pr-3 pl-[18px]">
        <h1 className="m-0 text-[13px] font-medium">My tickets</h1>
        <fieldset aria-label="Tabs" className="m-0 ml-2 flex gap-1 border-0 p-0">
          {MY_TABS.map(({ key, label }) => (
            <button
              key={key}
              type="button"
              aria-pressed={key === tab}
              onClick={() => setChosen(key)}
              className={`h-6 cursor-pointer rounded-[5px] border px-[9px] text-[12px] font-medium ${key === tab ? "border-btn-line bg-sel text-text" : "border-transparent bg-transparent text-text2 hover:text-text"}`}
            >
              {label} <span className="text-text3">{counts[key]}</span>
            </button>
          ))}
        </fieldset>
        <div className="ml-auto flex gap-[6px]">
          <Button kind="primary" onClick={() => overlays.open("ticket")}>
            New ticket <Kbd onAccent>N</Kbd>
          </Button>
        </div>
      </header>
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          {!supported ? (
            <p className="m-0 p-[18px] text-text3">This server does not have My tickets yet.</p>
          ) : mine.error ? (
            <p role="alert" className="m-0 p-[18px] text-red">
              {mine.error.message}
            </p>
          ) : shown.length === 0 && !mine.isPending ? (
            <p className="m-0 p-[18px] text-text3">{empty}</p>
          ) : (
            <TicketRows
              tickets={shown}
              columnName={columnName}
              selected={ticket}
              needs={need.needs}
              mine={need.mine}
              onOpen={open}
            />
          )}
          {supported ? (
            <button
              type="button"
              aria-pressed={includeDone}
              onClick={() => setIncludeDone(!includeDone)}
              className="flex h-[34px] w-full cursor-pointer items-center gap-2 border-0 border-b border-solid border-line2 bg-transparent px-[18px] font-medium text-text2 hover:text-text"
            >
              {includeDone ? "Hide Done" : "Show Done"}
            </button>
          ) : null}
        </div>
        {ticket ? <TicketPanel key={ticket} id={ticket} onClose={close} /> : null}
      </div>
    </>
  );
}
