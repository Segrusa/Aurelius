import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import { useMemo, useState } from "react";
import { useChannels, useEmployees, useGuidelines, useProjects, useTickets } from "../data";
import { hidden } from "../hidden";
import { keysFor, SHORTCUTS, showChord } from "../keys/keymap";
import { isMac, useKeyContext, useKeys } from "../keys/provider";
import { usePlatform } from "../platform";
import { SearchIcon } from "../ui/icons";
import { initial, Kbd, Mark, markColor } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { bareName } from "./chat-logic";
import { overlays, useOpenOverlay } from "./overlays-store";
import { filterGroups, type PaletteGroup } from "./palette-logic";

// The table's label says what the key does on every page; here it is one command.
const PALETTE_LABEL: Record<string, string> = { NewItem: "Give work" };

const COLUMN: Record<string, string> = {
  backlog: "Backlog",
  todo: "To do",
  doing: "Doing",
  testing: "Testing",
  review: "Review",
  done: "Done",
};

/** Only mounted while open, so a closed palette reads nothing and costs nothing. */
function PaletteBody({ onClose }: { onClose: () => void }) {
  useKeyContext("Palette", true);
  const keys = useKeys();
  const { kind } = usePlatform();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const projects = useProjects();
  const employees = useEmployees();
  const tickets = useTickets();
  const guidelines = useGuidelines();
  const channels = useChannels();

  const groups = useMemo<PaletteGroup[]>(() => {
    const go = (to: string) => () => void navigate({ to });
    const commands = SHORTCUTS.filter((s) => s.inPalette && keys.has(s.action) && !(s.feature && hidden(s.feature)));
    const shown = new Set<string>();
    return [
      {
        label: "Projects",
        items: (projects.data ?? []).map((p) => ({
          id: `project:${p.id}`,
          label: p.name,
          detail: "Open project",
          mark: p.id,
          run: go("/board"),
        })),
      },
      {
        label: "Tickets",
        items: (tickets.data ?? []).map((t) => ({
          id: `ticket:${t.id}`,
          label: `#${t.number} ${t.title}`,
          detail: [COLUMN[t.column] ?? t.column, t.owner_name].filter(Boolean).join(" · "),
          mark: t.id,
          run: () => void navigate({ to: "/board", search: { ticket: t.id } }),
        })),
      },
      {
        label: "Channels",
        items: (channels.data ?? []).map((c) => ({
          id: `channel:${c.conversation_id}`,
          label: `Go to #${bareName(c)}`,
          detail: "Team chat",
          mark: c.conversation_id,
          run: () =>
            void navigate({
              to: "/chat",
              search: c.kind && c.kind !== "general" ? { channel: c.conversation_id } : {},
            }),
        })),
      },
      {
        label: "Employees",
        items: (employees.data ?? []).map((e) => ({
          id: `employee:${e.id}`,
          label: e.name,
          detail: e.title ?? e.role,
          mark: e.id,
          run: go("/employees"),
        })),
      },
      {
        label: "Guidelines",
        items: (guidelines.data ?? [])
          .filter((g) => !g.archived)
          .map((g) => ({
            id: `guideline:${g.id}`,
            label: g.title,
            detail: `v${g.version}`,
            mark: g.id,
            run: go("/knowledge"),
          })),
      },
      {
        label: "Commands",
        items: commands
          .filter((s) => {
            if (shown.has(s.action)) return false;
            shown.add(s.action);
            return s.action !== "OpenPalette";
          })
          .map((s) => {
            const key = keysFor(s.keys, kind)[0];
            return {
              id: `command:${s.action}`,
              label: PALETTE_LABEL[s.action] ?? s.label,
              detail: "",
              mark: "",
              ...(key ? { keys: showChord(key, isMac) } : {}),
              run: () => keys.run(s.action),
            };
          }),
      },
    ];
  }, [projects.data, tickets.data, employees.data, guidelines.data, channels.data, keys, kind, navigate]);

  const shown = useMemo(() => filterGroups(query, groups), [query, groups]);

  return (
    <Command shouldFilter={false} label="Command palette" className="flex min-h-0 flex-col">
      <div className="flex h-[52px] flex-none items-center gap-[10px] border-b border-line2 px-4 text-text3">
        <SearchIcon />
        <Command.Input
          data-autofocus
          value={query}
          onValueChange={setQuery}
          aria-label="Search a project, employee or command"
          placeholder="Search a project, employee or command"
          className="min-w-0 flex-1 border-0 bg-transparent text-[15px] text-text outline-none placeholder:text-text3"
        />
        <Kbd>esc</Kbd>
      </div>
      <Command.List className="max-h-[400px] overflow-y-auto p-[6px]">
        <Command.Empty className="p-4 text-text3">Nothing found.</Command.Empty>
        {shown.map((group) => (
          <Command.Group
            key={group.label}
            heading={group.label}
            className="[&_[cmdk-group-heading]]:mx-[10px] [&_[cmdk-group-heading]]:mt-[6px] [&_[cmdk-group-heading]]:mb-1 [&_[cmdk-group-heading]]:text-[12px] [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-text3"
          >
            {group.items.map((item) => (
              <Command.Item
                key={item.id}
                value={item.id}
                onSelect={() => {
                  onClose();
                  item.run();
                }}
                className="flex h-9 cursor-pointer items-center gap-[10px] rounded-[7px] px-[10px] data-[selected=true]:bg-sel"
              >
                {item.mark ? (
                  <Mark
                    letter={item.label.startsWith("#") ? "#" : initial(item.label)}
                    color={markColor(item.mark)}
                    size={20}
                  />
                ) : (
                  <Mark letter="→" color="var(--acc)" size={20} />
                )}
                <span className="min-w-0 truncate font-medium">{item.label}</span>
                {item.detail ? <span className="min-w-0 truncate text-text3">{item.detail}</span> : null}
                {item.keys ? <span className="ml-auto flex-none text-[11px] text-text2">{item.keys}</span> : null}
              </Command.Item>
            ))}
          </Command.Group>
        ))}
      </Command.List>
      <div className="flex h-9 flex-none items-center gap-[14px] border-t border-line2 px-4 text-[12px] text-text3">
        <span>
          <Kbd>↑↓</Kbd> select
        </span>
        <span>
          <Kbd>↵</Kbd> open
        </span>
      </div>
    </Command>
  );
}

export function Palette() {
  const open = useOpenOverlay() === "palette";
  const { close } = overlays;
  return (
    <Dialog open={open} onClose={close} title="Command palette" top>
      {open ? <PaletteBody onClose={close} /> : null}
    </Dialog>
  );
}
