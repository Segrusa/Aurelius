import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Layout, LayoutContent, LayoutHeader } from "@astryxdesign/core/Layout";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import type { ColumnKind } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useLinkState } from "../connection";
import { useBoard, useMyTickets } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { KeyHint } from "../ui/key-hint";
import { Menu } from "../ui/menu";
import { useWorkspace } from "../workspace";
import { inMyTab, MY_TABS, type MyTab } from "./board-logic";
import { Loading, Page, Problem } from "./org-kit";
import { overlays } from "./overlays-store";
import { type KindFilter, kindFilterOptions, kindMatches, quickEnabled } from "./quick-logic";
import { TicketPanel } from "./ticket-panel";
import { TicketRows } from "./ticket-rows";
import { useTicketNeeds } from "./tickets-kit";

const styles = stylex.create({
  head: { display: "flex", minHeight: 44, alignItems: "center", gap: 10, paddingInlineStart: 18, paddingInlineEnd: 12 },
  title: { margin: 0, fontSize: 13, fontWeight: 500 },
  tabCount: { color: "var(--precision-text3)" },
  actions: { marginInlineStart: "auto", display: "flex", gap: 6 },
  body: { position: "relative", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  fill: { position: "absolute", inset: 0 },
  main: { display: "flex", height: "100%", minWidth: 0, flexDirection: "column" },
  message: { padding: 18 },
  showDone: {
    display: "flex",
    flexShrink: 0,
    height: 34,
    width: "100%",
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderWidth: 0,
    borderBottomWidth: 1,
    borderStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: "transparent",
    paddingInline: 18,
    fontWeight: 500,
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
});

export function MyTicketsScreen({ ticket }: { ticket: string | undefined }) {
  useKeyContext("MyTickets");
  const navigate = useNavigate();
  const { features, user } = useLinkState();
  const supported = features.includes("my_tickets");
  const [includeDone, setIncludeDone] = useState(false);
  const [chosen, setChosen] = useState<MyTab | null>(null);
  const workspace = useWorkspace();
  const mine = useMyTickets(workspace.id, includeDone, supported);
  const board = useBoard(null);
  const quickOn = quickEnabled(features);
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  // A server without quick tasks has no filter for them: every ticket shows.
  const kind: KindFilter = quickOn ? kindFilter : "all";
  const tickets = useMemo(() => (mine.data?.tickets ?? []).filter((t) => kindMatches(kind, t)), [mine.data, kind]);
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
    <Page>
      <LayoutHeader hasDivider padding={0} data-titlebar>
        <div {...stylex.props(styles.head)}>
          <Heading level={1} xstyle={styles.title}>
            My tickets
          </Heading>
          <ToggleButtonGroup
            label="Tabs"
            type="single"
            size="sm"
            value={tab}
            onChange={(next) => {
              if (next) setChosen(next as MyTab);
            }}
          >
            {MY_TABS.map(({ key, label }) => (
              <ToggleButton key={key} value={key} label={label}>
                {label} <span {...stylex.props(styles.tabCount)}>{counts[key]}</span>
              </ToggleButton>
            ))}
          </ToggleButtonGroup>
          <div {...stylex.props(styles.actions)}>
            {quickOn ? (
              <Menu
                label="Kind"
                value={kind}
                options={kindFilterOptions(true)}
                onPick={setKindFilter}
                trigger={
                  <Button variant="ghost" label={kindFilterOptions(true).find((o) => o.key === kind)?.label ?? ""} />
                }
              />
            ) : null}
            <Button variant="primary" label="New ticket" onClick={() => overlays.open("ticket")}>
              New ticket <KeyHint onAccent>N</KeyHint>
            </Button>
          </div>
        </div>
      </LayoutHeader>
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.fill)}>
          <Layout
            height="fill"
            end={ticket ? <TicketPanel key={ticket} id={ticket} onClose={close} /> : null}
            content={
              <LayoutContent isScrollable={false} padding={0}>
                <div {...stylex.props(styles.main)}>
                  {!supported ? (
                    <div {...stylex.props(styles.message)}>
                      <EmptyState isCompact title="This server does not have My tickets yet." />
                    </div>
                  ) : mine.error ? (
                    <div {...stylex.props(styles.message)}>
                      <Problem>{mine.error.message}</Problem>
                    </div>
                  ) : mine.isPending && shown.length === 0 ? (
                    <Loading rows={6} />
                  ) : shown.length === 0 ? (
                    <div {...stylex.props(styles.message)}>
                      <EmptyState isCompact title={empty ?? ""} />
                    </div>
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
                      {...stylex.props(styles.showDone)}
                    >
                      {includeDone ? "Hide Done" : "Show Done"}
                    </button>
                  ) : null}
                </div>
              </LayoutContent>
            }
          />
        </div>
      </div>
    </Page>
  );
}
