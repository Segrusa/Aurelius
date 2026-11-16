// The side panel of a conversation that does work: a task's run and a work session show the same
// properties, files, guidelines and usage, so both read them from here.

import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { useRunGuidelines } from "../data";
import type { UsageTotals } from "./run-logic";
import { tokensShort } from "./usage-logic";

const styles = stylex.create({
  aside: {
    display: { default: "flex", "@media (max-width: 900px)": "none" },
    width: "clamp(240px, 24%, 340px)",
    flexShrink: 0,
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 20,
    overflowY: "auto",
    borderLeftWidth: 1,
    borderLeftStyle: "solid",
    borderLeftColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    padding: 18,
  },
  title: { marginTop: 0, marginBottom: 8, fontSize: 12, fontWeight: 500, color: "var(--precision-text3)" },
  section: {
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingTop: 16,
  },
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
  usage: { fontSize: 12, lineHeight: 1.7, color: "var(--precision-text3)" },
});

/** The panel itself, right of the conversation; hidden on a narrow window. */
export function RunAside({ label, children }: { label: string; children: ReactNode }) {
  return (
    <aside aria-label={label} {...stylex.props(styles.aside)}>
      {children}
    </aside>
  );
}

/** The first block: name and value rows under "Properties". */
export function SideProperties({ children }: { children: ReactNode }) {
  return (
    <div>
      <p {...stylex.props(styles.title)}>Properties</p>
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

/** A later block, with a line above it. */
export function SideSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.section)}>
      <p {...stylex.props(styles.title)}>{title}</p>
      {children}
    </div>
  );
}

export function FilesWritten({ files }: { files: string[] }) {
  if (files.length === 0) return null;
  return (
    <SideSection title="Files written">
      {files.map((f) => (
        <div key={f} title={f} {...stylex.props(styles.file)}>
          {f}
        </div>
      ))}
    </SideSection>
  );
}

/** The guidelines the server put in that run's system text (`run.guidelines`). */
export function RunGuidelines({ runId }: { runId: string | null }) {
  const guidelines = useRunGuidelines(runId);
  const list = guidelines.data?.guidelines ?? [];
  if (list.length === 0) return null;
  return (
    <SideSection title="Guidelines in this run">
      {list.map((g) => (
        <div key={g.guideline_id} {...stylex.props(styles.guideline)}>
          <span {...stylex.props(styles.truncate)}>{g.title}</span>
          <span {...stylex.props(styles.version)}>v{g.version}</span>
        </div>
      ))}
    </SideSection>
  );
}

export function UsageTotalsLine({ usage }: { usage: UsageTotals }) {
  if (usage.input + usage.output === 0) return null;
  return (
    <div {...stylex.props(styles.section, styles.usage)}>
      {tokensShort(usage.input)} in · {tokensShort(usage.output)} out
      <br />
      {tokensShort(usage.cached)} cache read · {tokensShort(usage.written)} write
      {usage.micro > 0 ? (
        <>
          <br />≈ ${(usage.micro / 1_000_000).toFixed(2)} API-equivalent
        </>
      ) : null}
    </div>
  );
}
