import { Heading } from "@astryxdesign/core/Heading";
import { LayoutHeader } from "@astryxdesign/core/Layout";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useEffect } from "react";
import { useLinkState } from "../connection";
import { type Feature, shown } from "../hidden";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Icon } from "../ui/icons";
import { ActivitySection } from "./activity";
import type { ActivityFilters } from "./activity-logic";
import {
  BackupsSection,
  CompactionSection,
  ConnectionsSection,
  LoadSection,
  McpSection,
  SchedulesSection,
  TraceSection,
} from "./settings-advanced";
import { EmployeeRunsSection } from "./settings-employee-runs";
import { MyDefaultsSection } from "./settings-run-defaults";
import { focusField, HINTS_KEY, pressKeyed, stepRow } from "./settings-kit";
import { CompanySection, RemoteSection, ServerSection } from "./settings-workspace";
import { AccountSection, AppSection } from "./settings-you";

/** Keys that press a control on the page shown; the control says so with `Keyed`. */
const KEYED_ACTIONS = [
  "SettingsVerify",
  "SettingsUse",
  "SettingsKey",
  "SettingsNew",
  "SettingsRemove",
  "SettingsCheckUpdate",
  "SettingsInstallUpdate",
  "SettingsInstallAppUpdate",
  "SettingsRestartServer",
  "SettingsShutdownNow",
  "SettingsShutdownIdle",
  "SettingsBackupNow",
  "SettingsLoginStart",
  "SettingsShowHints",
  "SettingsRestore",
  "SettingsResume",
  "SettingsRemoteToggle",
  "SettingsRefusePairing",
  "SettingsPushTest",
  "SettingsCopyPairing",
  "LoadSave",
  "LoadToggleEnabled",
];

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  sidebar: {
    display: "flex",
    width: 224,
    flexShrink: 0,
    flexDirection: "column",
    gap: 18,
    overflowY: "auto",
    paddingBlockStart: 6,
    paddingInlineEnd: 10,
    paddingBlockEnd: 10,
    paddingInlineStart: 14,
  },
  back: {
    display: "flex",
    height: 32,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    paddingInline: 8,
    fontWeight: 500,
    textDecoration: "none",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  nav: { display: "flex", flexDirection: "column", gap: 14 },
  group: { display: "flex", flexDirection: "column", gap: 1 },
  groupName: {
    margin: 0,
    marginBlockEnd: 4,
    paddingInline: 8,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--precision-text3)",
  },
  link: {
    display: "flex",
    height: 28,
    flexShrink: 0,
    alignItems: "center",
    borderRadius: 6,
    paddingInline: 8,
    fontWeight: 500,
    textDecoration: "none",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  linkOn: { backgroundColor: "var(--color-overlay-hover)", color: "var(--color-text-primary)" },
  place: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    marginBlockStart: "auto",
    paddingInline: 8,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  dot: {
    width: 6,
    height: 6,
    flexShrink: 0,
    borderRadius: "50%",
    backgroundColor: "var(--color-text-accent)",
  },
  placeName: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  head: {
    display: "flex",
    height: 44,
    alignItems: "center",
    gap: 8,
    paddingInlineStart: 18,
    paddingInlineEnd: 12,
    color: "var(--precision-text3)",
  },
  arrow: { color: "var(--color-text-disabled)" },
  title: { margin: 0, fontSize: 13, fontWeight: 500 },
  scroller: { minHeight: 0, flex: 1, overflowY: "auto" },
  wide: { maxWidth: 1180 },
  page: {
    display: "flex",
    width: "100%",
    maxWidth: 820,
    boxSizing: "border-box",
    flexDirection: "column",
    gap: 28,
    paddingBlock: 28,
    paddingInline: { default: 24, "@media (min-width: 1280px)": 40 },
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
  },
  missing: { margin: 0, color: "var(--precision-text3)" },
});

interface Section {
  key: string;
  label: string;
  /** What the section shows inside Settings; a section without one is a page of its own at `to`. */
  body?: (filters: ActivityFilters) => ReactNode;
  /** The page's own route, for a page too big for a Settings section (Employees, Knowledge). */
  to?: string;
  feature?: Feature;
  /** A table that needs the room. */
  wide?: boolean;
}

// hidden.ts leads: what it names stays out.
const ALL_GROUPS: { label: string; sections: Section[] }[] = [
  {
    label: "You",
    sections: [
      { key: "account", label: "Account", body: () => <AccountSection /> },
      { key: "app", label: "App", body: () => <AppSection /> },
      { key: "run-defaults", label: "My defaults", body: () => <MyDefaultsSection /> },
    ],
  },
  {
    label: "Company",
    sections: [
      { key: "company", label: "Company", body: () => <CompanySection /> },
      { key: "employees", label: "Employees", to: "/employees" },
      { key: "knowledge", label: "Knowledge", to: "/knowledge" },
    ],
  },
  {
    label: "Workspace",
    sections: [
      { key: "employee-runs", label: "Model & effort", body: () => <EmployeeRunsSection /> },
      { key: "servers", label: "Servers & PC", body: () => <ServerSection /> },
      { key: "remote", label: "Remote access", body: () => <RemoteSection /> },
    ],
  },
  {
    label: "Advanced",
    sections: [
      { key: "connections", label: "Connections", body: () => <ConnectionsSection /> },
      { key: "mcp", label: "MCP & tools", body: () => <McpSection /> },
      { key: "schedules", label: "Recurring schedules", body: () => <SchedulesSection />, feature: "RecurringTasks" },
      { key: "backups", label: "Backups", body: () => <BackupsSection /> },
      { key: "load", label: "Load limits", body: () => <LoadSection /> },
      { key: "compaction", label: "Context compaction", body: () => <CompactionSection /> },
      { key: "trace", label: "Trace", body: () => <TraceSection />, feature: "Trace" },
      { key: "activity", label: "Activity log", body: (filters) => <ActivitySection filters={filters} />, wide: true },
    ],
  },
];

const GROUPS = ALL_GROUPS.map((group) => ({ ...group, sections: shown(group.sections) }));
/** The sections Settings itself shows; Employees and Knowledge are pages of their own. */
const SECTIONS = GROUPS.flatMap((g) => g.sections).filter((s) => s.body);

// The list of projects is a tab of Knowledge.
function companyPageOn(to: string, path: string): boolean {
  return path === to || path.startsWith(`${to}/`) || (to === "/knowledge" && path === "/projects");
}

export function SettingsSidebar() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const { hostName, remote } = useLinkState();
  return (
    <aside {...stylex.props(styles.sidebar)}>
      <Link to="/new" {...stylex.props(styles.back)}>
        <Icon name="back" size={14} />
        Back to the app
      </Link>
      <nav aria-label="Settings" {...stylex.props(styles.nav)}>
        {GROUPS.map((group) => (
          <div key={group.label} {...stylex.props(styles.group)}>
            <p {...stylex.props(styles.groupName)}>{group.label}</p>
            {group.sections.map((section) => {
              if (section.to) {
                const on = companyPageOn(section.to, path);
                return (
                  <Link
                    key={section.key}
                    to={section.to}
                    aria-current={on ? "page" : undefined}
                    {...stylex.props(styles.link, on && styles.linkOn)}
                  >
                    {section.label}
                  </Link>
                );
              }
              const on = path === `/settings/${section.key}`;
              return (
                <Link
                  key={section.key}
                  to="/settings/$section"
                  params={{ section: section.key }}
                  aria-current={on ? "page" : undefined}
                  {...stylex.props(styles.link, on && styles.linkOn)}
                >
                  {section.label}
                </Link>
              );
            })}
          </div>
        ))}
      </nav>
      <div {...stylex.props(styles.place)}>
        <span {...stylex.props(styles.dot)} />
        <span {...stylex.props(styles.placeName)}>
          {remote ? "Remote" : "This computer"} · {hostName ?? "Aurelius"}
        </span>
      </div>
    </aside>
  );
}

export function SettingsScreen({ section, filters }: { section: string; filters: ActivityFilters }) {
  const navigate = useNavigate();
  useKeyContext("Settings");
  const { storage } = usePlatform();
  useEffect(() => {
    document.documentElement.toggleAttribute("data-hints", storage.get(HINTS_KEY) === "true");
  }, [storage]);
  useAction("SettingsRenameServer", () => focusField("Server name"));
  useAction("SettingsPushKey", () => focusField("Team id"));
  useAction("SelectNext", () => stepRow(1));
  useAction("SelectPrev", () => stepRow(-1));
  for (const action of KEYED_ACTIONS) {
    // biome-ignore lint/correctness/useHookAtTopLevel: a fixed list: the same hooks in the same order.
    useAction(action, () => pressKeyed(action));
  }
  useAction("Back", () => void navigate({ to: "/new" }));
  const stepSection = (by: number) => {
    const at = SECTIONS.findIndex((s) => s.key === section);
    const next = SECTIONS[(at + by + SECTIONS.length) % SECTIONS.length];
    if (next) void navigate({ to: "/settings/$section", params: { section: next.key } });
  };
  useAction("NextTab", () => stepSection(1));
  useAction("PrevTab", () => stepSection(-1));
  const found = SECTIONS.find((s) => s.key === section);
  return (
    <>
      <LayoutHeader hasDivider padding={0} data-titlebar>
        <div {...stylex.props(styles.head)}>
          Settings<span {...stylex.props(styles.arrow)}>›</span>
          <Heading level={1} xstyle={styles.title}>
            {found?.label ?? "Not found"}
          </Heading>
        </div>
      </LayoutHeader>
      <div {...stylex.props(styles.scroller)}>
        <div key={section} {...stylex.props(styles.page, found?.wide && styles.wide)}>
          {found?.body ? found.body(filters) : <p {...stylex.props(styles.missing)}>This section does not exist.</p>}
        </div>
      </div>
    </>
  );
}
