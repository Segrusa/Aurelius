// The Inbox behind a bell at the right of every screen's head: a count of what needs you, and a
// dropdown with the latest rows of each group. A row opens on the Inbox page, where it is decided.

import { Button } from "@astryxdesign/core/Button";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Popover } from "@astryxdesign/core/Popover";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { Icon, StatusGlyph } from "../ui/icons";
import { INBOX_TONES, useInbox } from "./inbox-data";
import { age, type InboxRow, needsYouCount } from "./inbox-logic";

/** Rows per group in the dropdown; the Inbox page has the rest. */
const ROWS_PER_GROUP = 4;

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
  const { groups } = useInbox();
  const [open, setOpen] = useState(false);
  const needs = needsYouCount(groups);
  const rows = groups.reduce((n, g) => n + g.rows.length, 0);
  const go = (row: InboxRow | null) => {
    setOpen(false);
    void navigate({ to: "/inbox", search: row ? { row: row.id } : {} });
  };
  const now = Date.now();
  const label = needs > 0 ? `Inbox, ${needs} need you` : "Inbox";
  return (
    <div {...stylex.props(styles.place)}>
      <Popover
        label="Inbox"
        isOpen={open}
        onOpenChange={setOpen}
        width={380}
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
                <Button size="sm" variant="ghost" label="Open Inbox" onClick={() => go(null)} />
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
                        {g.rows.slice(0, ROWS_PER_GROUP).map((row) => (
                          <button key={row.id} type="button" onClick={() => go(row)} {...stylex.props(styles.row)}>
                            <span {...stylex.props(styles.glyph)}>
                              <StatusGlyph glyph={row.glyph} color={INBOX_TONES[row.tone]} />
                            </span>
                            <span {...stylex.props(styles.words)}>
                              <span {...stylex.props(styles.title)}>{row.title}</span>
                              {row.meta ? <span {...stylex.props(styles.meta)}>{row.meta}</span> : null}
                            </span>
                            <span {...stylex.props(styles.age)}>{age(row.atMs, now)}</span>
                          </button>
                        ))}
                        {g.rows.length > ROWS_PER_GROUP ? (
                          <p {...stylex.props(styles.more)}>{g.rows.length - ROWS_PER_GROUP} more in the Inbox</p>
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
