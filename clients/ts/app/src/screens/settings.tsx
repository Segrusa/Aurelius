import { Heading } from "@astryxdesign/core/Heading";
import { LayoutHeader } from "@astryxdesign/core/Layout";
import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { type Feature, shown } from "../hidden";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Icon, type NavIcon, SearchIcon } from "../ui/icons";
import { ActivitySection } from "./activity";
import type { ActivityFilters } from "./activity-logic";
import { BackupsSection, ConnectionsSection, McpSection, RunsAndLoadSection } from "./settings-advanced";
import { EmployeeRunsSection } from "./settings-employee-runs";
import { focusField, HINTS_KEY, pressKeyed, stepRow, useSetting } from "./settings-kit";
import { sectionKeyOf, sectionsFor, settingMatches } from "./settings-logic";
import { MyDefaultsSection, USER_RUN_DEFAULTS_FEATURE } from "./settings-run-defaults";
import { BoardMembersSection, CompanySection, RemoteSection, ServerSection } from "./settings-workspace";
import { AccountSection, DeviceSection } from "./settings-you";

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
    width: 236,
    flexShrink: 0,
    flexDirection: "column",
    minHeight: 0,
    borderInlineEndWidth: 1,
    borderInlineEndStyle: "solid",
    borderInlineEndColor: "var(--color-border)",
  },
  top: { display: "flex", flexDirection: "column", gap: 8, paddingBlock: "10px 6px", paddingInline: 12 },
  back: {
    display: "flex",
    height: 30,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    paddingInline: 8,
    fontWeight: 500,
    textDecoration: "none",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  search: {
    display: "flex",
    height: 30,
    alignItems: "center",
    gap: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 7,
    paddingInline: 9,
    backgroundColor: "var(--color-background-card)",
    color: "var(--precision-text3)",
  },
  searchInput: {
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
  kbd: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: 4,
    paddingInline: 4,
    fontFamily: "var(--font-family-code)",
    fontSize: 11,
    color: "var(--precision-text3)",
  },
  nav: {
    display: "flex",
    minHeight: 0,
    flex: 1,
    flexDirection: "column",
    gap: 14,
    overflowY: "auto",
    paddingBlock: "6px 12px",
    paddingInline: 12,
  },
  group: { display: "flex", flexDirection: "column", gap: 1 },
  groupName: {
    margin: 0,
    marginBlockEnd: 3,
    paddingInline: 8,
    fontSize: 11.5,
    fontWeight: 600,
    color: "var(--precision-text3)",
  },
  link: {
    display: "flex",
    height: 30,
    flexShrink: 0,
    alignItems: "center",
    gap: 9,
    borderRadius: 6,
    paddingInline: 8,
    fontWeight: 500,
    textDecoration: "none",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  linkOn: { backgroundColor: "var(--color-overlay-pressed)", color: "var(--color-text-primary)" },
  icon: { display: "flex", color: "var(--precision-text3)" },
  iconOn: { color: "var(--color-text-primary)" },
  label: { minWidth: 0, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  flag: { width: 7, height: 7, flexShrink: 0, borderRadius: "50%" },
  flagwarn: { backgroundColor: "var(--color-warning)" },
  flaginfo: { backgroundColor: "var(--color-info)" },
  none: { margin: 0, paddingBlock: 4, paddingInline: 8, color: "var(--precision-text3)" },
  place: {
    display: "flex",
    alignItems: "center",
    gap: 7,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingBlock: "10px 12px",
    paddingInline: 20,
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
    paddingInline: 20,
    color: "var(--precision-text3)",
  },
  title: { margin: 0, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  scroller: { minHeight: 0, flex: 1, overflowY: "auto" },
  wide: { maxWidth: 1080 },
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
  missing: { margin: 0, color: "var(--precision-text3)" },
});

type Flag = "warn" | "info";

interface Section {
  key: string;
  label: string;
  icon: NavIcon;
  /** More words Search settings finds this entry by. */
  keywords: string;
  /** What the section shows inside Settings; a section without one is a page of its own at `to`. */
  body?: (filters: ActivityFilters) => ReactNode;
  /** The page's own route, for a page too big for a Settings section (Employees, Knowledge). */
  to?: string;
  /** Other paths that light this entry up. */
  also?: string[];
  feature?: Feature;
  /** A table that needs the room. */
  wide?: boolean;
  /** A server feature that replaced this section: it shows only on a server without it. */
  replacedBy?: string;
}

// hidden.ts leads: what it names stays out.
const ALL_GROUPS: { label: string; sections: Section[] }[] = [
  {
    label: "You",
    sections: [
      {
        key: "account",
        label: "Account",
        icon: "user",
        keywords: "profile password sign out claude login devices sessions pair phone",
        body: () => <AccountSection />,
      },
      {
        key: "defaults",
        label: "My defaults",
        icon: "sliders",
        keywords: "model effort mode plan auto ask subagent",
        body: () => <MyDefaultsSection />,
      },
      // My defaults replaced it (OPR-5 point 5); an old server still keeps model and effort per employee.
      {
        key: "employee-runs",
        label: "Model & effort",
        icon: "sliders",
        keywords: "model effort employee",
        body: () => <EmployeeRunsSection />,
        replacedBy: USER_RUN_DEFAULTS_FEATURE,
      },
      {
        key: "device",
        label: "This device",
        icon: "laptop",
        keywords: "app appearance dark light theme shortcut hints start at sign-in background update version",
        body: () => <DeviceSection />,
      },
    ],
  },
  {
    label: "Company",
    sections: [
      {
        key: "company",
        label: "General",
        icon: "building",
        keywords: "company name management mode org chart tester testing background jobs memory consolidation",
        body: () => <CompanySection />,
      },
      {
        key: "members",
        label: "Board members",
        icon: "team",
        keywords: "users add disable sessions owner",
        body: () => <BoardMembersSection />,
      },
      { key: "employees", label: "Employees", icon: "grid", keywords: "team org", to: "/employees" },
      { key: "workspaces", label: "Workspaces", icon: "folder", keywords: "projects", to: "/projects" },
      {
        key: "guidelines",
        label: "Guidelines",
        icon: "book",
        keywords: "rules knowledge",
        to: "/knowledge/guidelines",
        also: ["/knowledge/vault"],
      },
      { key: "memory", label: "Memory", icon: "book", keywords: "remember knowledge", to: "/knowledge/memory" },
    ],
  },
  {
    label: "Server",
    sections: [
      {
        key: "server",
        label: "Overview",
        icon: "server",
        keywords: "server name update version restart shut down power pc other servers",
        body: () => <ServerSection />,
      },
      {
        key: "remote",
        label: "Remote & phones",
        icon: "globe",
        keywords: "remote access tailscale pair device phone notifications apns push",
        body: () => <RemoteSection />,
      },
      {
        key: "connections",
        label: "Connections",
        icon: "plug",
        keywords: "api key models company connection",
        body: () => <ConnectionsSection />,
      },
      {
        key: "tools",
        label: "Tools & MCP",
        icon: "tool",
        keywords: "mcp servers browser always allowed safe",
        body: () => <McpSection />,
      },
      {
        key: "load",
        label: "Runs & load",
        icon: "usage",
        keywords: "load limits cpu memory runs at once compaction context tokens",
        body: () => <RunsAndLoadSection />,
      },
      {
        key: "backups",
        label: "Backups",
        icon: "archive",
        keywords: "restore back up",
        body: () => <BackupsSection />,
      },
      {
        key: "activity",
        label: "Activity log",
        icon: "list",
        keywords: "audit history",
        body: (filters) => <ActivitySection filters={filters} />,
        wide: true,
      },
    ],
  },
];

const GROUPS = ALL_GROUPS.map((group) => ({ ...group, sections: shown(group.sections) }));
/** The sections Settings itself shows; Employees and Knowledge are pages of their own. */
const SECTIONS = GROUPS.flatMap((g) => g.sections).filter((s) => s.body);

function companyPageOn(section: Section, path: string): boolean {
  const under = (root: string) => path === root || path.startsWith(`${root}/`);
  return [section.to ?? "", ...(section.also ?? [])].some((root) => root !== "" && under(root));
}

/** A dot on an entry that needs you: no login for background jobs, or a server update. */
function useFlags(): Record<string, Flag> {
  const connection = useConnection();
  const company = useSetting("company", async () => expectBody(await connection.call("company.get"), "company").value);
  const update = useSetting(
    "update",
    async () => expectBody(await connection.call("update.get", { check: false }), "update").value,
  );
  const flags: Record<string, Flag> = {};
  if (company.data && !company.data.background_login) flags.company = "warn";
  if (update.data?.enabled && (update.data.available || update.data.staged)) flags.server = "info";
  return flags;
}

const FLAG_TITLES: Record<Flag, string> = { warn: "Needs you", info: "Update available" };

function NavEntry({ section, path, flag }: { section: Section; path: string; flag: Flag | undefined }) {
  const on = section.to ? companyPageOn(section, path) : path === `/settings/${section.key}`;
  const inner = (
    <>
      <span {...stylex.props(styles.icon, on && styles.iconOn)}>
        <Icon name={section.icon} size={15} />
      </span>
      <span {...stylex.props(styles.label)}>{section.label}</span>
      {flag ? (
        <span aria-hidden="true" title={FLAG_TITLES[flag]} {...stylex.props(styles.flag, styles[`flag${flag}`])} />
      ) : null}
    </>
  );
  if (section.to) {
    return (
      <Link
        to={section.to}
        aria-current={on ? "page" : undefined}
        aria-description={flag ? FLAG_TITLES[flag] : undefined}
        {...stylex.props(styles.link, on && styles.linkOn)}
      >
        {inner}
      </Link>
    );
  }
  return (
    <Link
      to="/settings/$section"
      params={{ section: section.key }}
      aria-current={on ? "page" : undefined}
      aria-description={flag ? FLAG_TITLES[flag] : undefined}
      {...stylex.props(styles.link, on && styles.linkOn)}
    >
      {inner}
    </Link>
  );
}

/** Whether a key press is typed into a field rather than meant as a shortcut. */
function typing(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))
  );
}

export function SettingsSidebar() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const { hostName, remote, features } = useLinkState();
  const flags = useFlags();
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.ctrlKey || event.metaKey || event.altKey || typing(event.target)) return;
      if (document.querySelector("dialog[open]")) return;
      event.preventDefault();
      search.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const groups = GROUPS.map((group) => ({
    ...group,
    sections: sectionsFor(group.sections, features).filter((s) => settingMatches(s.label, s.keywords, query)),
  })).filter((group) => group.sections.length > 0);
  return (
    <aside {...stylex.props(styles.sidebar)}>
      <div {...stylex.props(styles.top)}>
        <Link to="/new" {...stylex.props(styles.back)}>
          <Icon name="back" size={14} />
          Back to the app
        </Link>
        <label {...stylex.props(styles.search)}>
          <SearchIcon />
          <input
            ref={search}
            type="search"
            placeholder="Search settings"
            aria-label="Search settings"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                setQuery("");
                e.currentTarget.blur();
              }
            }}
            {...stylex.props(styles.searchInput)}
          />
          <kbd {...stylex.props(styles.kbd)}>/</kbd>
        </label>
      </div>
      <nav aria-label="Settings" {...stylex.props(styles.nav)}>
        {groups.map((group) => (
          <div key={group.label} {...stylex.props(styles.group)}>
            <p {...stylex.props(styles.groupName)}>{group.label}</p>
            {group.sections.map((section) => (
              <NavEntry key={section.key} section={section} path={path} flag={flags[section.key]} />
            ))}
          </div>
        ))}
        {groups.length === 0 ? <p {...stylex.props(styles.none)}>Nothing found.</p> : null}
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
  const { features } = useLinkState();
  const sections = sectionsFor(SECTIONS, features);
  const stepSection = (by: number) => {
    const at = sections.findIndex((s) => s.key === sectionKeyOf(section));
    const next = sections[(at + by + sections.length) % sections.length];
    if (next) void navigate({ to: "/settings/$section", params: { section: next.key } });
  };
  useAction("NextTab", () => stepSection(1));
  useAction("PrevTab", () => stepSection(-1));
  // An old link opens the page its section moved to; a replaced section opens what replaced it
  // (Model & effort → My defaults).
  const key = sectionKeyOf(section);
  const found =
    sections.find((s) => s.key === key) ??
    (SECTIONS.some((s) => s.key === key) ? sections.find((s) => s.key === "defaults") : undefined);
  return (
    <>
      <LayoutHeader hasDivider padding={0} data-titlebar>
        <div {...stylex.props(styles.head)}>
          Settings<span>›</span>
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
