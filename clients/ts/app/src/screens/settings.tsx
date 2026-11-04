import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useLinkState } from "../connection";
import { useAction } from "../keys/provider";
import { Icon } from "../ui/icons";
import {
  BackupsSection,
  ConnectionsSection,
  LoadSection,
  McpSection,
  SchedulesSection,
  TraceSection,
} from "./settings-advanced";
import { CompanySection, RemoteSection, ServerSection } from "./settings-workspace";
import { AccountSection, AppSection } from "./settings-you";

interface Section {
  key: string;
  label: string;
  body: () => ReactNode;
}

// Plugins and App keys are in the design's list, but hidden.ts leads: they stay out.
const GROUPS: { label: string; sections: Section[] }[] = [
  {
    label: "You",
    sections: [
      { key: "account", label: "Account", body: () => <AccountSection /> },
      { key: "app", label: "App", body: () => <AppSection /> },
    ],
  },
  {
    label: "Workspace",
    sections: [
      { key: "company", label: "Company", body: () => <CompanySection /> },
      { key: "servers", label: "Servers & PC", body: () => <ServerSection /> },
      { key: "remote", label: "Remote access", body: () => <RemoteSection /> },
    ],
  },
  {
    label: "Advanced",
    sections: [
      { key: "connections", label: "Connections", body: () => <ConnectionsSection /> },
      { key: "mcp", label: "MCP & tools", body: () => <McpSection /> },
      { key: "schedules", label: "Recurring schedules", body: () => <SchedulesSection /> },
      { key: "backups", label: "Backups", body: () => <BackupsSection /> },
      { key: "load", label: "Load limits", body: () => <LoadSection /> },
      { key: "trace", label: "Trace", body: () => <TraceSection /> },
    ],
  },
];

export function SettingsSidebar() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const { hostName, remote } = useLinkState();
  return (
    <aside className="flex w-[224px] flex-none flex-col gap-[18px] overflow-y-auto pt-[6px] pr-[10px] pb-[10px] pl-[14px]">
      <Link
        to="/inbox"
        className="flex h-8 flex-none items-center gap-2 px-2 font-medium text-text2 no-underline hover:text-text"
      >
        <Icon name="back" size={14} />
        Back to the app
      </Link>
      <nav aria-label="Settings" className="flex flex-col gap-[14px]">
        {GROUPS.map((group) => (
          <div key={group.label} className="flex flex-col gap-px">
            <p className="m-0 mb-1 px-2 text-[12px] font-medium text-text3">{group.label}</p>
            {group.sections.map((section) => {
              const to = `/settings/${section.key}`;
              const on = path === to;
              return (
                <Link
                  key={section.key}
                  to="/settings/$section"
                  params={{ section: section.key }}
                  aria-current={on ? "page" : undefined}
                  className={`flex h-7 flex-none items-center rounded-[6px] px-2 font-medium no-underline ${on ? "bg-sel text-text" : "text-text2 hover:bg-sel hover:text-text"}`}
                >
                  {section.label}
                </Link>
              );
            })}
          </div>
        ))}
      </nav>
      <div className="mt-auto flex items-center gap-[6px] px-2 text-[12px] text-text3">
        <span className="size-[6px] flex-none rounded-full bg-acc-text" />
        <span className="truncate">
          {remote ? "Remote" : "This computer"} · {hostName ?? "Aurelius"}
        </span>
      </div>
    </aside>
  );
}

export function SettingsScreen({ section }: { section: string }) {
  const navigate = useNavigate();
  useAction("Back", () => void navigate({ to: "/inbox" }));
  const stepSection = (by: number) => {
    const all = GROUPS.flatMap((g) => g.sections);
    const at = all.findIndex((s) => s.key === section);
    const next = all[(at + by + all.length) % all.length];
    if (next) void navigate({ to: "/settings/$section", params: { section: next.key } });
  };
  useAction("NextTab", () => stepSection(1));
  useAction("PrevTab", () => stepSection(-1));
  const found = GROUPS.flatMap((g) => g.sections).find((s) => s.key === section);
  return (
    <>
      <header className="flex h-11 flex-none items-center gap-2 border-b border-line pr-3 pl-[18px] text-text3">
        Settings<span className="text-faint">›</span>
        <h1 className="m-0 text-[13px] font-medium text-text">{found?.label ?? "Not found"}</h1>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div key={section} className="enter box-border flex w-full max-w-[820px] flex-col gap-7 px-6 py-7 xl:px-10">
          {found ? found.body() : <p className="m-0 text-text3">This section does not exist.</p>}
        </div>
      </div>
    </>
  );
}
