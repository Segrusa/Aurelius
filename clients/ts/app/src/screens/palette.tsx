import {
  CommandPalette,
  CommandPaletteFooter,
  CommandPaletteInput,
  useCommandPaletteContext,
} from "@astryxdesign/core/CommandPalette";
import type { SearchableItem, SearchSource } from "@astryxdesign/core/Typeahead";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef } from "react";
import { useLinkState } from "../connection";
import { useChannels, useEmployees, useGuidelines, useProjects, useTickets } from "../data";
import { hidden } from "../hidden";
import { keysFor, SHORTCUTS, showChord } from "../keys/keymap";
import { isMac, useKeyContext, useKeys } from "../keys/provider";
import { usePlatform } from "../platform";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { useWorkspace } from "../workspace";
import { bareName } from "./chat-logic";
import { overlays, useOpenOverlay } from "./overlays-store";
import { filterGroups, type PaletteGroup, quickTaskCommands } from "./palette-logic";
import { quickEnabled } from "./quick-logic";
import { useSwitchWorkspace } from "./workspace-kit";
import { guidelinesIn, narrow, openProjects } from "./workspace-logic";

// The table's label says what the key does on every page; here it is one command.
const PALETTE_LABEL: Record<string, string> = { NewItem: "Give work" };

const styles = stylex.create({
  label: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: 500 },
  detail: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--precision-text3)",
  },
  keys: { marginInlineStart: "auto", flexShrink: 0, fontSize: 11, color: "var(--color-text-secondary)" },
  hint: { display: "flex", alignItems: "center", gap: 4 },
});

interface EntryData {
  group: string;
  detail: string;
  mark: string;
  keys: string;
}

interface Entry extends SearchableItem<EntryData> {
  auxiliaryData: EntryData;
}

function entries(groups: PaletteGroup[]): Entry[] {
  return groups.flatMap((group) =>
    group.items.map((item) => ({
      id: item.id,
      label: item.label,
      auxiliaryData: { group: group.label, detail: item.detail, mark: item.mark, keys: item.keys ?? "" },
    })),
  );
}

function renderEntry(item: Entry) {
  const { detail, mark, keys } = item.auxiliaryData;
  return (
    <>
      {mark ? (
        <Mark letter={item.label.startsWith("#") ? "#" : initial(item.label)} color={markColor(mark)} size={20} />
      ) : (
        <Mark letter="→" color="var(--color-accent)" size={20} />
      )}
      <span {...stylex.props(styles.label)}>{item.label}</span>
      {detail ? <span {...stylex.props(styles.detail)}>{detail}</span> : null}
      {keys ? <span {...stylex.props(styles.keys)}>{keys}</span> : null}
    </>
  );
}

/** The first row is what Enter takes; what loads while the palette is open comes into the list. */
function PaletteInput({ groups }: { groups: PaletteGroup[] }) {
  const context = useCommandPaletteContext();
  const results = context?.searchResults;
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new result set moves the highlight.
  useEffect(() => {
    if (results && results.length > 0) context?.setHighlightedIndex(0);
  }, [results]);
  const loaded = useRef(groups);
  // biome-ignore lint/correctness/useExhaustiveDependencies: only new data searches again.
  useEffect(() => {
    if (loaded.current === groups) return;
    loaded.current = groups;
    context?.setSearch(context.search);
  }, [groups]);
  return (
    <CommandPaletteInput
      label="Command palette"
      placeholder="Search a workspace, ticket, employee or command"
      onKeyDown={(e) => {
        // Results arrive in a transition: an Enter right after typing finds nothing highlighted yet.
        if (e.key !== "Enter" || e.nativeEvent.isComposing || context?.highlightedIndex !== -1) return;
        const first = filterGroups(e.currentTarget.value, groups)[0]?.items[0];
        if (first) context.selectItem(first.id);
      }}
      endContent={<KeyHint>esc</KeyHint>}
    />
  );
}

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
  const projects = useProjects();
  const { openProjectWindow } = usePlatform();
  const employees = useEmployees();
  const tickets = useTickets();
  const guidelines = useGuidelines();
  const channels = useChannels();
  const workspace = useWorkspace();
  const switchTo = useSwitchWorkspace();
  const quick = quickEnabled(useLinkState().features);
  // The workspace's id as the only thing the search depends on: `switchTo` changes every render.
  const switchRef = useRef(switchTo);
  switchRef.current = switchTo;

  const groups = useMemo<PaletteGroup[]>(() => {
    const go = (to: string) => () => void navigate({ to });
    const here = workspace.id;
    const commands = SHORTCUTS.filter((s) => s.inPalette && keys.has(s.action) && !(s.feature && hidden(s.feature)));
    const shown = new Set<string>();
    return [
      ...(workspace.available && !workspace.locked
        ? [
            {
              label: "Workspaces",
              items: [
                {
                  id: "workspace:all",
                  label: "All projects",
                  detail: "Switch workspace",
                  mark: "",
                  run: () => switchRef.current(null),
                },
                ...openProjects(projects.data ?? []).map((p) => ({
                  id: `workspace:${p.id}`,
                  label: p.name,
                  detail: "Switch workspace",
                  mark: p.id,
                  run: () => switchRef.current(p.id),
                })),
                ...(here
                  ? [
                      {
                        id: "workspace:settings",
                        label: "Workspace settings",
                        detail: workspace.project?.name ?? "",
                        mark: here,
                        run: () => void navigate({ to: "/projects/$id", params: { id: here } }),
                      },
                    ]
                  : []),
              ],
            },
          ]
        : [
            {
              label: "Projects",
              items: (projects.data ?? []).map((p) => ({
                id: `project:${p.id}`,
                label: p.name,
                detail: "Open project",
                mark: p.id,
                run: () => void navigate({ to: "/projects/$id", params: { id: p.id } }),
              })),
            },
          ]),
      ...(openProjectWindow
        ? [
            {
              label: "Project windows",
              items: (projects.data ?? []).map((p) => ({
                id: `project-window:${p.id}`,
                label: `Open ${p.name} in a new window`,
                detail: "Project window",
                mark: p.id,
                run: () => void openProjectWindow(p.id, p.name),
              })),
            },
          ]
        : []),
      {
        label: "Tickets",
        items: narrow(tickets.data ?? [], here).map((t) => ({
          id: `ticket:${t.id}`,
          label: `#${t.number} ${t.title}`,
          detail: [COLUMN[t.column] ?? t.column, t.owner_name].filter(Boolean).join(" · "),
          mark: t.id,
          run: () => void navigate({ to: "/board", search: { ticket: t.id } }),
        })),
      },
      {
        label: "Channels",
        items: narrow(channels.data ?? [], here).map((c) => ({
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
        items: guidelinesIn(guidelines.data ?? [], here, employees.data ?? [])
          .filter((g) => !g.archived)
          .map((g) => ({
            id: `guideline:${g.id}`,
            label: g.title,
            detail: `v${g.version}`,
            mark: g.id,
            run: () => void navigate({ to: "/knowledge/guidelines/$id", params: { id: g.id } }),
          })),
      },
      {
        label: "Commands",
        items: [
          ...quickTaskCommands(quick, () => overlays.open("ticket", { title: "", description: "", quick: true })),
          ...commands
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
        ],
      },
    ];
  }, [
    workspace,
    openProjectWindow,
    projects.data,
    tickets.data,
    employees.data,
    guidelines.data,
    channels.data,
    keys,
    kind,
    navigate,
    quick,
  ]);

  const itemsById = useMemo(() => new Map(groups.flatMap((g) => g.items.map((i) => [i.id, i]))), [groups]);
  // Never the whole list of tickets: each search asks filterGroups, which caps every group.
  const source = useMemo<SearchSource<Entry>>(
    () => ({
      search: (query) => entries(filterGroups(query, groups)),
      bootstrap: () => entries(filterGroups("", groups)),
    }),
    [groups],
  );
  // Picking also closes the palette, afterwards; that must not close what the pick just opened.
  const picked = useRef(false);

  return (
    <CommandPalette
      isOpen
      onOpenChange={(open) => {
        if (!open && !picked.current) onClose();
      }}
      searchSource={source}
      renderItem={renderEntry}
      onValueChange={(id) => {
        picked.current = true;
        onClose();
        itemsById.get(id)?.run();
      }}
      input={<PaletteInput groups={groups} />}
      emptySearchText="Nothing found."
      footer={
        <CommandPaletteFooter>
          <span {...stylex.props(styles.hint)}>
            <KeyHint>↑↓</KeyHint> select
          </span>
          <span {...stylex.props(styles.hint)}>
            <KeyHint>↵</KeyHint> open
          </span>
        </CommandPaletteFooter>
      }
      // Forwarded to its Dialog: results that come and go must not make the palette jump around.
      {...({ position: { top: "14vh" } } as object)}
    />
  );
}

export function Palette() {
  const open = useOpenOverlay() === "palette";
  const { close } = overlays;
  return open ? <PaletteBody onClose={close} /> : null;
}
