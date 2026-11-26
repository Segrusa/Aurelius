// The frame every Settings page shares (design/settings-redesign.html and settings-company.html):
// the "Settings › … › Title" bar, the centred page, and the pieces of the Company pages
// (toolbar, find field, segments, status, tabs, save bar, meter).

import { Heading } from "@astryxdesign/core/Heading";
import { LayoutHeader } from "@astryxdesign/core/Layout";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { SearchIcon } from "../ui/icons";

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const pulse = stylex.keyframes({
  "0%": { boxShadow: "0 0 0 0 var(--color-accent-muted)" },
  "50%": { boxShadow: "0 0 0 3px var(--color-accent-muted)" },
  "100%": { boxShadow: "0 0 0 0 var(--color-accent-muted)" },
});

const styles = stylex.create({
  frame: { display: "flex", minHeight: 0, flex: 1, flexDirection: "column" },
  head: {
    display: "flex",
    height: 44,
    alignItems: "center",
    gap: 8,
    paddingInline: 20,
    color: "var(--precision-text3)",
  },
  crumb: {
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
    textDecoration: "none",
  },
  title: {
    minWidth: 0,
    margin: 0,
    overflow: "hidden",
    fontSize: 13,
    fontWeight: 600,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--color-text-primary)",
  },
  headActions: { display: "flex", alignItems: "center", gap: 8, marginInlineStart: "auto" },
  scroller: { minHeight: 0, flex: 1, overflowY: "auto" },
  page: {
    display: "flex",
    width: "100%",
    maxWidth: 720,
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 26,
    marginInline: "auto",
    paddingBlock: "28px 64px",
    paddingInline: 28,
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
    "@media (prefers-reduced-motion: reduce)": { animationName: "none" },
  },
  mid: { maxWidth: 920 },
  wide: { maxWidth: 1080 },
  toolbar: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  grow: { flex: 1 },
  find: {
    display: "flex",
    width: 260,
    height: 30,
    boxSizing: "border-box",
    alignItems: "center",
    gap: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 7,
    paddingInline: 9,
    backgroundColor: "var(--precision-sunk)",
    color: "var(--precision-text3)",
  },
  findInput: {
    minWidth: 0,
    flex: 1,
    borderWidth: 0,
    padding: 0,
    outlineStyle: "none",
    backgroundColor: "transparent",
    color: "var(--color-text-primary)",
    font: "inherit",
    "::placeholder": { color: "var(--precision-text3)" },
  },
  seg: {
    display: "inline-flex",
    gap: 2,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 8,
    padding: 2,
    backgroundColor: "var(--precision-sunk)",
  },
  segButton: {
    height: 24,
    cursor: "pointer",
    borderWidth: 0,
    borderRadius: 6,
    paddingInline: 10,
    backgroundColor: "transparent",
    fontSize: 12.5,
    fontWeight: 500,
    whiteSpace: "nowrap",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  segOn: {
    backgroundColor: "var(--color-background-card)",
    boxShadow: "0 1px 2px rgb(0 0 0 / 0.14)",
    color: "var(--color-text-primary)",
  },
  status: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    fontSize: 12.5,
    whiteSpace: "nowrap",
    color: "var(--color-text-secondary)",
  },
  statusyou: { fontWeight: 500, color: "var(--color-text-warning)" },
  statuspaused: { color: "var(--precision-text3)" },
  dot: { width: 7, height: 7, flexShrink: 0, borderRadius: "50%", backgroundColor: "var(--precision-text3)" },
  dotwork: {
    backgroundColor: "var(--color-text-accent)",
    animationName: pulse,
    animationDuration: "1.6s",
    animationIterationCount: "infinite",
    "@media (prefers-reduced-motion: reduce)": { animationName: "none" },
  },
  dotyou: { backgroundColor: "var(--color-text-warning)" },
  dotwait: { backgroundColor: "var(--color-text-info)" },
  dotpaused: {
    width: 8,
    height: 8,
    boxSizing: "border-box",
    borderWidth: 1.5,
    borderStyle: "solid",
    borderColor: "var(--precision-text3)",
    backgroundColor: "transparent",
  },
  tabs: {
    display: "flex",
    gap: 2,
    marginBlockStart: -10,
    overflowX: "auto",
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--color-border)",
    scrollbarWidth: "none",
  },
  tab: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    marginBlockEnd: -1,
    cursor: "pointer",
    borderWidth: 0,
    borderBlockEndWidth: 2,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "transparent",
    paddingBlock: 8,
    paddingInline: 12,
    backgroundColor: "transparent",
    fontWeight: 500,
    whiteSpace: "nowrap",
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  tabOn: { borderBlockEndColor: "var(--color-text-primary)", color: "var(--color-text-primary)" },
  count: { fontSize: 11, color: "var(--precision-text3)" },
  countWarn: { fontWeight: 600, color: "var(--color-text-warning)" },
  savebar: {
    position: "sticky",
    insetBlockEnd: 12,
    display: "flex",
    alignItems: "center",
    gap: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: 10,
    paddingBlock: 8,
    paddingInlineStart: 14,
    paddingInlineEnd: 8,
    backgroundColor: "var(--color-background-card)",
    boxShadow: "0 8px 24px rgb(0 0 0 / 0.12)",
    zIndex: 1,
  },
  saveText: { flex: 1, color: "var(--color-text-secondary)" },
  meter: {
    height: 6,
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 999,
    backgroundColor: "var(--precision-sunk)",
  },
  meterFill: { display: "block", height: "100%", borderRadius: 999, backgroundColor: "var(--color-text-accent)" },
  meterWarn: { backgroundColor: "var(--color-warning)" },
  lede: { margin: 0, marginBlockStart: -14, paddingInline: 2, fontSize: 12.5, color: "var(--precision-text3)" },
});

export interface Crumb {
  label: string;
  to: string;
  /** A page with unsaved work asks first: it calls preventDefault to stay. */
  onClick?: (event: React.MouseEvent<HTMLAnchorElement>) => void;
}

/** The bar on top of a Settings page: "Settings › Employees › Flux", with actions at the right. */
export function SettingsHead({
  title,
  crumbs = [],
  children,
}: {
  title: ReactNode;
  crumbs?: Crumb[];
  children?: ReactNode;
}) {
  return (
    <LayoutHeader hasDivider padding={0} data-titlebar>
      <div {...stylex.props(styles.head)}>
        Settings<span>›</span>
        {crumbs.map((c) => (
          <span key={c.to} {...stylex.props(styles.toolbar)}>
            <Link to={c.to} {...(c.onClick ? { onClick: c.onClick } : {})} {...stylex.props(styles.crumb)}>
              {c.label}
            </Link>
            <span>›</span>
          </span>
        ))}
        <Heading level={1} xstyle={styles.title}>
          {title}
        </Heading>
        {children ? <div {...stylex.props(styles.headActions)}>{children}</div> : null}
      </div>
    </LayoutHeader>
  );
}

export type PageWidth = "narrow" | "mid" | "wide";

/** The scrolling area with the page centred in it: 720, 920 or 1080 wide. */
export function SettingsBody({
  width = "narrow",
  pageKey,
  children,
}: {
  width?: PageWidth;
  /** A new key replays the page's entrance, as a change of page does. */
  pageKey?: string;
  children: ReactNode;
}) {
  return (
    <div {...stylex.props(styles.scroller)}>
      <div key={pageKey} {...stylex.props(styles.page, width === "mid" && styles.mid, width === "wide" && styles.wide)}>
        {children}
      </div>
    </div>
  );
}

/** A Settings page outside /settings (Employees, Guidelines, Memory, Workspaces): its bar and its body. */
export function SettingsPage({
  title,
  crumbs,
  width,
  actions,
  children,
}: {
  title: ReactNode;
  crumbs?: Crumb[];
  width?: PageWidth;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div {...stylex.props(styles.frame)}>
      <SettingsHead title={title} {...(crumbs ? { crumbs } : {})}>
        {actions}
      </SettingsHead>
      <SettingsBody {...(width ? { width } : {})}>{children}</SettingsBody>
    </div>
  );
}

/** A row of controls above a list: find, filters, the view and the main action. */
export function Toolbar({ children }: { children: ReactNode }) {
  return <div {...stylex.props(styles.toolbar)}>{children}</div>;
}

export function Grow() {
  return <span {...stylex.props(styles.grow)} />;
}

/** The small grey sentence under a page's bar. */
export function Lede({ children }: { children: ReactNode }) {
  return <p {...stylex.props(styles.lede)}>{children}</p>;
}

/** A find field with its loupe, as in a toolbar. */
export function FindField({
  label,
  placeholder,
  value,
  onChange,
  grow = false,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
  grow?: boolean;
}) {
  return (
    <label {...stylex.props(styles.find)} style={grow ? { flex: 1, minWidth: 160, width: "auto" } : undefined}>
      <SearchIcon />
      <input
        type="search"
        aria-label={label}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        {...stylex.props(styles.findInput)}
      />
    </label>
  );
}

/** Buttons of which one is on: a filter or a view. */
export function Segments<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a group of toggle buttons, as the design draws it.
    <div role="group" aria-label={label} {...stylex.props(styles.seg)}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
          {...stylex.props(styles.segButton, o.value === value && styles.segOn)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export type StatusTone = "work" | "you" | "wait" | "paused" | "plain";

const DOTS = {
  work: styles.dotwork,
  you: styles.dotyou,
  wait: styles.dotwait,
  paused: styles.dotpaused,
  plain: null,
} as const;
const TEXTS = { you: styles.statusyou, paused: styles.statuspaused, work: null, wait: null, plain: null } as const;

/** "● Working": an employee's state as a dot and a word. Without a label only the dot shows. */
export function Status({ tone, label, dotOnly = false }: { tone: StatusTone; label: string; dotOnly?: boolean }) {
  return (
    <span {...stylex.props(styles.status, TEXTS[tone])}>
      <span
        {...stylex.props(styles.dot, DOTS[tone])}
        {...(dotOnly ? { role: "img", "aria-label": label } : { "aria-hidden": true })}
      />
      {dotOnly ? null : label}
    </span>
  );
}

/** The tabs of a page (an employee's Work, Profile, …), each with an optional count. */
export function PageTabs<T extends string>({
  label,
  tabs,
  value,
  onChange,
}: {
  label: string;
  tabs: { value: T; label: string; count?: number; warn?: boolean }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div role="tablist" aria-label={label} {...stylex.props(styles.tabs)}>
      {tabs.map((t) => (
        <button
          key={t.value}
          type="button"
          role="tab"
          aria-selected={t.value === value}
          onClick={() => onChange(t.value)}
          {...stylex.props(styles.tab, t.value === value && styles.tabOn)}
        >
          {t.label}
          {t.count ? <span {...stylex.props(styles.count, t.warn && styles.countWarn)}>{t.count}</span> : null}
        </button>
      ))}
    </div>
  );
}

/** The bar that stays at the bottom while a form has changes: what changed, Discard and Save. */
export function SaveBar({ children, actions }: { children: ReactNode; actions: ReactNode }) {
  return (
    <div role="status" {...stylex.props(styles.savebar)}>
      <span {...stylex.props(styles.saveText)}>{children}</span>
      {actions}
    </div>
  );
}

/** How full something is: guidelines against their limit, memory against its budget. */
export function Meter({ used, limit, label }: { used: number; limit: number; label: string }) {
  const share = limit > 0 ? Math.min(1, used / limit) : 0;
  return (
    // biome-ignore lint/a11y/useSemanticElements: a <meter> cannot be drawn as the design's bar in every browser.
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={limit}
      aria-valuenow={used}
      {...stylex.props(styles.meter)}
    >
      <i {...stylex.props(styles.meterFill, share > 0.9 && styles.meterWarn)} style={{ width: `${share * 100}%` }} />
    </div>
  );
}
