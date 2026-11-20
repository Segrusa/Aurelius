// The properties of a conversation that does work: a task's run, a work session and a ticket show
// their properties, files, guidelines and usage from here. The head spans the whole screen; below it,
// the properties dock as a card beside the conversation, shown or hidden with the head's button. On a
// narrow window the panel slides in as a sheet over the conversation instead.

import { Drawer } from "@astryxdesign/core/Drawer";
import { Icon as AstryxIcon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useState, useSyncExternalStore } from "react";
import { useRunGuidelines } from "../data";
import { usePlatform } from "../platform";
import { Icon } from "../ui/icons";
import type { UsageTotals } from "./run-logic";
import { tokensShort } from "./usage-logic";

/** Remembered for every screen with a properties panel: who hides it once wants it hidden everywhere. */
const OPEN_KEY = "run-side.properties.open";
/** Below this the panel is a sheet over the conversation; between it and `DOCKED_OPEN_W` it starts closed. */
const SHEET_BELOW_W = 900;
const DOCKED_OPEN_W = 1100;
const SHEET_W = 360;

const styles = stylex.create({
  // The column beside the conversation; the card inside it holds the properties.
  aside: {
    display: "flex",
    width: "clamp(270px, 26%, 350px)",
    minHeight: 0,
    flexShrink: 0,
    boxSizing: "border-box",
    flexDirection: "column",
    paddingBlock: 12,
    paddingInlineEnd: 12,
  },
  // A ticket's panel holds text to edit, so it may be a little wider.
  wide: { width: "clamp(290px, 28%, 390px)" },
  // A card set apart by its shadow, as high as its content and scrolling past the window's height.
  card: {
    display: "flex",
    maxHeight: "100%",
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 4,
    overflowY: "auto",
    borderRadius: 12,
    backgroundColor: "var(--color-background-popover)",
    boxShadow: "var(--shadow-high)",
    padding: 16,
  },
  // In the sheet of a narrow window the sheet is the card.
  sheet: { width: "100%", minHeight: "100%", padding: 0 },
  sheetCard: { minHeight: "100%", maxHeight: "none", borderRadius: 0, boxShadow: "none", padding: 18 },
  // A block's heading reads as a heading, above rows whose names are grey: that is the distinction.
  title: { marginTop: 0, marginBottom: 6, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  // Blocks are set apart by space, as in a card; no lines between them.
  section: { marginTop: 16 },
  // The heading of a block that folds: the whole row is the button.
  toggle: {
    display: "flex",
    width: "calc(100% + 12px)",
    cursor: "pointer",
    alignItems: "center",
    gap: 6,
    marginInline: -6,
    borderRadius: 6,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingBlock: 4,
    paddingInline: 6,
    textAlign: "left",
    fontSize: 13,
    fontWeight: 600,
    color: "var(--color-text-primary)",
  },
  toggleOpen: { marginBottom: 4 },
  toggleTitle: { flexGrow: 1 },
  count: { fontWeight: 400, color: "var(--precision-text3)" },
  chevron: { display: "flex", color: "var(--precision-text3)", transition: "transform 120ms ease" },
  chevronClosed: { transform: "rotate(-90deg)" },
  row: { display: "grid", minHeight: 30, gridTemplateColumns: "100px minmax(0,1fr)", alignItems: "center" },
  rowName: { color: "var(--precision-text3)" },
  rowValue: { minWidth: 0, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  file: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    paddingBlock: 4,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  guideline: { display: "flex", justifyContent: "space-between", gap: 8, paddingBlock: 4 },
  truncate: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  version: { flexShrink: 0, color: "var(--precision-text3)" },
  usage: { fontSize: 12, lineHeight: 1.7 },
  titleRow: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8, marginBottom: 6 },
  titleInRow: { marginBottom: 0 },
});

/** Whether the window is narrower than `px`, following resizes. */
function useNarrower(px: number): boolean {
  const query = `(max-width: ${px - 1}px)`;
  return useSyncExternalStore(
    (changed) => {
      const list = matchMedia(query);
      list.addEventListener("change", changed);
      return () => list.removeEventListener("change", changed);
    },
    () => matchMedia(query).matches,
  );
}

/** Whether a screen's properties panel shows, shared by its head button and the panel itself. */
export interface PropertiesDock {
  narrow: boolean;
  shown: boolean;
  toggle: () => void;
  closeSheet: () => void;
}

/** The remembered choice wins; a first open docks the panel only on a wide window. */
export function usePropertiesDock(): PropertiesDock {
  const { storage } = usePlatform();
  const narrow = useNarrower(SHEET_BELOW_W);
  const [docked, setDocked] = useState(() => {
    const kept = storage.get(OPEN_KEY);
    return kept === null ? window.innerWidth >= DOCKED_OPEN_W : kept !== "0";
  });
  const [sheet, setSheet] = useState(false);
  return {
    narrow,
    shown: narrow ? sheet : docked,
    toggle: () => {
      if (narrow) return setSheet(!sheet);
      storage.set(OPEN_KEY, docked ? "0" : "1");
      setDocked(!docked);
    },
    closeSheet: () => setSheet(false),
  };
}

/** The head's button that shows or hides the panel. */
export function PropertiesToggle({ dock, label }: { dock: PropertiesDock; label: string }) {
  return (
    <IconButton
      variant="ghost"
      size="sm"
      label={`${dock.shown ? "Hide" : "Show"} ${label.toLowerCase()}`}
      aria-expanded={dock.shown}
      icon={<Icon name="properties" size={16} />}
      onClick={dock.toggle}
    />
  );
}

/** The panel beside the conversation, or the sheet over it on a narrow window. */
export function PropertiesPanel({
  dock,
  label,
  wide = false,
  children,
}: {
  dock: PropertiesDock;
  label: string;
  /** A ticket's properties (ticket #70). */
  wide?: boolean;
  children: ReactNode;
}) {
  if (dock.narrow) {
    return (
      <Drawer
        isOpen={dock.shown}
        onOpenChange={(open) => (open ? undefined : dock.closeSheet())}
        label={label}
        width={SHEET_W}
        padding={0}
      >
        <aside aria-label={label} {...stylex.props(styles.aside, styles.sheet)}>
          <div {...stylex.props(styles.card, styles.sheetCard)}>{children}</div>
        </aside>
      </Drawer>
    );
  }
  if (!dock.shown) return null;
  return (
    <aside aria-label={label} {...stylex.props(styles.aside, wide && styles.wide)}>
      <div {...stylex.props(styles.card)}>{children}</div>
    </aside>
  );
}

/** The first block: name and value rows under "Properties". */
export function SideProperties({ children, usage = null }: { children: ReactNode; usage?: UsageTotals | null }) {
  return (
    <div>
      <div {...stylex.props(styles.titleRow)}>
        <p {...stylex.props(styles.title, styles.titleInRow)}>Properties</p>
        {usage ? <UsageInfo usage={usage} /> : null}
      </div>
      {children}
    </div>
  );
}

export function SideRow({ name, title, children }: { name: string; title?: string; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.rowName)}>{name}</span>
      <span title={title} {...stylex.props(styles.rowValue)}>
        {children}
      </span>
    </div>
  );
}

/** A later block, with space above it. */
export function SideSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.section)}>
      <p {...stylex.props(styles.title)}>{title}</p>
      {children}
    </div>
  );
}

/** A later block that starts folded: a long list that is only now and then read. */
export function FoldedSection({ title, count, children }: { title: string; count: number; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div {...stylex.props(styles.section)}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        {...stylex.props(styles.toggle, open && styles.toggleOpen)}
      >
        <span {...stylex.props(styles.chevron, !open && styles.chevronClosed)}>
          <Icon name="chevronDown" size={14} />
        </span>
        <span {...stylex.props(styles.toggleTitle)}>{title}</span>
        <span {...stylex.props(styles.count)}>{count}</span>
      </button>
      {open ? children : null}
    </div>
  );
}

export function FilesWritten({ files }: { files: string[] }) {
  if (files.length === 0) return null;
  return (
    <FoldedSection title="Files written" count={files.length}>
      {files.map((f) => (
        <div key={f} title={f} {...stylex.props(styles.file)}>
          {f}
        </div>
      ))}
    </FoldedSection>
  );
}

/** The guidelines the server put in that run's system text (`run.guidelines`). */
export function RunGuidelines({ runId, title = "Guidelines in this run" }: { runId: string | null; title?: string }) {
  const guidelines = useRunGuidelines(runId);
  const list = guidelines.data?.guidelines ?? [];
  if (list.length === 0) return null;
  return (
    <FoldedSection title={title} count={list.length}>
      {list.map((g) => (
        <div key={g.guideline_id} {...stylex.props(styles.guideline)}>
          <span {...stylex.props(styles.truncate)}>{g.title}</span>
          <span {...stylex.props(styles.version)}>v{g.version}</span>
        </div>
      ))}
    </FoldedSection>
  );
}

/** What the runs used, behind an info icon beside the "Properties" heading: rarely read, so out of the way. */
function UsageInfo({ usage }: { usage: UsageTotals }) {
  if (usage.input + usage.output === 0) return null;
  return (
    <Tooltip
      placement="below"
      alignment="end"
      content={
        <div {...stylex.props(styles.usage)}>
          {tokensShort(usage.input)} in · {tokensShort(usage.output)} out
          <br />
          {tokensShort(usage.cached)} cache read · {tokensShort(usage.written)} write
          {usage.micro > 0 ? (
            <>
              <br />≈ ${(usage.micro / 1_000_000).toFixed(2)} API-equivalent
            </>
          ) : null}
        </div>
      }
    >
      <IconButton variant="ghost" size="sm" label="Usage" icon={<AstryxIcon icon="info" />} />
    </Tooltip>
  );
}
