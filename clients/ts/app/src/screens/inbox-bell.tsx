// The Inbox, behind a bell at the right of every screen's head: a count of what needs you, and a
// dropdown with every group's rows. There is no Inbox page: a row leads to where it is answered or
// decided (its ticket, run, conversation or message), and an approval with no such place is decided
// in the dropdown itself.

import { Button } from "@astryxdesign/core/Button";
import { Icon as AstryxIcon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Popover } from "@astryxdesign/core/Popover";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useNotifications, useWorkSessions } from "../data";
import { Icon, StatusGlyph } from "../ui/icons";
import { toast } from "../ui/toast";
import { ApprovalCard } from "./approval-card";
import { INBOX_TONES, useInbox } from "./inbox-data";
import { age, type InboxRow, needsYouCount, rowPlace } from "./inbox-logic";
import { useFollowLink } from "./links";
import { messageLink } from "./links-logic";
import { ticketPlace } from "./ticket-session-logic";

/** Rows per group in the dropdown; Clear all makes room for older updates. */
const ROWS_PER_GROUP = 50;

/** The bells that are on screen, so a key (Ctrl+I) can open one. */
const openers = new Set<() => void>();

/** Opens the bell's dropdown, wherever it stands in the head (the OpenNotifications action). */
export function openInboxBell(): void {
  for (const open of openers) open();
}

const styles = stylex.create({
  // Right in the head, whatever else the head holds.
  place: { position: "relative", display: "flex", flexShrink: 0, marginInlineStart: "auto" },
  badge: {
    position: "absolute",
    top: -2,
    right: -3,
    minWidth: 16,
    height: 16,
    boxSizing: "border-box",
    borderRadius: 8,
    backgroundColor: "var(--color-text-warning)",
    paddingInline: 4,
    textAlign: "center",
    fontSize: 10,
    fontWeight: 600,
    lineHeight: "16px",
    color: "var(--color-background-popover)",
    pointerEvents: "none",
  },
  surface: { borderRadius: 12, boxShadow: "var(--shadow-high)" },
  panel: { display: "flex", maxHeight: "min(70vh, 560px)", flexDirection: "column" },
  top: {
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    justifyContent: "space-between",
    paddingBlock: 10,
    paddingInline: 16,
  },
  heading: { margin: 0, fontSize: 13, fontWeight: 600 },
  actions: { display: "flex", alignItems: "center", gap: 4 },
  ask: { fontSize: 12, color: "var(--color-text-secondary)" },
  unseen: { fontWeight: 600 },
  // A row with its × beside it; an approval's card goes under both.
  line: { position: "relative" },
  dismiss: { position: "absolute", top: 6, right: 8 },
  roomForDismiss: { paddingInlineEnd: 44 },
  // An approval decided in the dropdown: its card under the row.
  decide: { paddingBottom: 10, paddingInline: 16 },
  list: { overflowY: "auto", paddingBottom: 8 },
  group: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    paddingTop: 10,
    paddingBottom: 4,
    paddingInline: 16,
    fontSize: 12,
    fontWeight: 600,
    color: "var(--precision-text3)",
  },
  dot: { width: 6, height: 6, borderRadius: 3 },
  count: { fontWeight: 400 },
  row: {
    display: "grid",
    width: "100%",
    cursor: "pointer",
    gridTemplateColumns: "16px minmax(0,1fr) auto",
    alignItems: "start",
    columnGap: 10,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingBlock: 7,
    paddingInline: 16,
    textAlign: "left",
    color: "var(--color-text-primary)",
  },
  glyph: { display: "flex", paddingTop: 2 },
  words: { display: "flex", minWidth: 0, flexDirection: "column", gap: 2 },
  title: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", fontSize: 13 },
  meta: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  age: { fontSize: 12, color: "var(--precision-text3)" },
  more: { margin: 0, paddingBlock: 4, paddingInline: 16, fontSize: 12, color: "var(--precision-text3)" },
  empty: { margin: 0, paddingBlock: 24, paddingInline: 16, textAlign: "center", color: "var(--precision-text3)" },
});

export function InboxBell() {
  const navigate = useNavigate();
  const connection = useConnection();
  const client = useQueryClient();
  const { hostId, features } = useLinkState();
  const follow = useFollowLink();
  const sessions = useWorkSessions().data ?? [];
  const notifications = useNotifications().data ?? [];
  const { groups } = useInbox();
  const [open, setOpen] = useState(false);
  const [deciding, setDeciding] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);
  useEffect(() => {
    const opener = () => setOpen(true);
    openers.add(opener);
    return () => {
      openers.delete(opener);
    };
  }, []);
  const needs = needsYouCount(groups);
  const rows = groups.reduce((n, g) => n + g.rows.length, 0);
  const unread = notifications.some((n) => !n.read);
  const clearable = notifications.some((n) => !n.needs_you);
  const write = async (
    method:
      | "notification.mark_read"
      | "notification.dismiss"
      | "notification.mark_all_read"
      | "notification.dismiss_all",
    params?: { id: string },
  ) => {
    try {
      await connection.write(method, params, { retry: false });
      await client.invalidateQueries({ queryKey: ["notifications"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const go = (row: InboxRow) => {
    const { source } = row;
    if (source.kind === "notification" && !source.notification.read) {
      void write("notification.mark_read", { id: source.notification.id });
    }
    const place = rowPlace(row, Boolean(hostId && follow && features.includes("message_locate")));
    switch (place.kind) {
      case "here":
        // An approval is decided right here; an update without a place is only read.
        if (source.kind === "approval") setDeciding(deciding === row.id ? null : row.id);
        return;
      case "ticket":
        void navigate(
          ticketPlace({ id: place.id, number: place.number }, place.message ? { message: place.message } : undefined),
        );
        break;
      case "task":
        void navigate({ to: "/tasks/$taskId", params: { taskId: place.id } });
        break;
      case "conversation":
        if (sessions.some((s) => s.conversation_id === place.id)) {
          void navigate({ to: "/session/$conversationId", params: { conversationId: place.id } });
        } else void navigate({ to: "/chat", search: { channel: place.id } });
        break;
      case "message":
        if (hostId && follow) follow(messageLink(hostId, place.id));
        break;
      case "employees":
        // Proposed hires wait on the org chart, where they are approved.
        void navigate({ to: "/employees/org" });
        break;
      case "guideline":
        void navigate({
          to: "/knowledge/guidelines/$id",
          params: { id: place.id },
          search: { proposal: place.proposal },
        });
        break;
    }
    setOpen(false);
  };
  const now = Date.now();
  const label = needs > 0 ? `Inbox, ${needs} need you` : "Inbox";
  return (
    <div {...stylex.props(styles.place)}>
      <Popover
        label="Inbox"
        isOpen={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) {
            setDeciding(null);
            setClearing(false);
          }
        }}
        width={400}
        padding={0}
        placement="below"
        alignment="end"
        xstyle={styles.surface}
        // Drawn only while open: a closed dropdown in every head would repeat the Inbox's text on the page.
        content={
          open ? (
            <div {...stylex.props(styles.panel)}>
              <div {...stylex.props(styles.top)}>
                <p {...stylex.props(styles.heading)}>Inbox</p>
                <div {...stylex.props(styles.actions)}>
                  {clearing ? (
                    <>
                      <span {...stylex.props(styles.ask)}>Clear every update?</span>
                      <Button
                        size="sm"
                        variant="destructive"
                        label="Clear"
                        onClick={() => {
                          setClearing(false);
                          void write("notification.dismiss_all");
                        }}
                      />
                      <Button size="sm" variant="ghost" label="Keep" onClick={() => setClearing(false)} />
                    </>
                  ) : (
                    <>
                      {unread ? (
                        <Button
                          size="sm"
                          variant="ghost"
                          label="Mark all read"
                          onClick={() => void write("notification.mark_all_read")}
                        />
                      ) : null}
                      {clearable ? (
                        <Button size="sm" variant="ghost" label="Clear all" onClick={() => setClearing(true)} />
                      ) : null}
                    </>
                  )}
                </div>
              </div>
              {rows === 0 ? (
                <p {...stylex.props(styles.empty)}>Nothing here. You are up to date.</p>
              ) : (
                <div {...stylex.props(styles.list)}>
                  {groups
                    .filter((g) => g.rows.length > 0)
                    .map((g) => (
                      <section key={g.key} aria-label={g.label}>
                        <div {...stylex.props(styles.group)}>
                          <span {...stylex.props(styles.dot)} style={{ background: INBOX_TONES[g.tone] }} />
                          {g.label}
                          <span {...stylex.props(styles.count)}>{g.rows.length}</span>
                        </div>
                        {g.rows.slice(0, ROWS_PER_GROUP).map((row) => {
                          const note = row.source.kind === "notification" ? row.source.notification : null;
                          const unseen = note !== null && !note.read;
                          return (
                            <div key={row.id} {...stylex.props(styles.line)}>
                              <button
                                type="button"
                                aria-expanded={row.source.kind === "approval" ? deciding === row.id : undefined}
                                onClick={() => go(row)}
                                {...stylex.props(styles.row, note !== null && styles.roomForDismiss)}
                              >
                                <span {...stylex.props(styles.glyph)}>
                                  <StatusGlyph glyph={row.glyph} color={INBOX_TONES[row.tone]} />
                                </span>
                                <span {...stylex.props(styles.words)}>
                                  <span {...stylex.props(styles.title, unseen && styles.unseen)}>{row.title}</span>
                                  {row.meta ? <span {...stylex.props(styles.meta)}>{row.meta}</span> : null}
                                </span>
                                <span {...stylex.props(styles.age)}>{age(row.atMs, now)}</span>
                              </button>
                              {note ? (
                                <span {...stylex.props(styles.dismiss)}>
                                  <IconButton
                                    variant="ghost"
                                    size="sm"
                                    label={`Dismiss ${row.title}`}
                                    icon={<AstryxIcon icon="close" />}
                                    onClick={() => void write("notification.dismiss", { id: note.id })}
                                  />
                                </span>
                              ) : null}
                              {deciding === row.id && row.source.kind === "approval" ? (
                                <div {...stylex.props(styles.decide)}>
                                  <ApprovalCard approval={row.source.approval} />
                                </div>
                              ) : null}
                            </div>
                          );
                        })}
                        {g.rows.length > ROWS_PER_GROUP ? (
                          <p {...stylex.props(styles.more)}>And {g.rows.length - ROWS_PER_GROUP} older</p>
                        ) : null}
                      </section>
                    ))}
                </div>
              )}
            </div>
          ) : null
        }
      >
        <IconButton variant="ghost" size="sm" label={label} icon={<Icon name="bell" size={16} />} />
      </Popover>
      {needs > 0 ? (
        <span aria-hidden {...stylex.props(styles.badge)}>
          {needs > 99 ? "99+" : needs}
        </span>
      ) : null}
    </div>
  );
}
