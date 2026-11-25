// Every action has a keyboard shortcut. This table is the single source of truth: it registers the
// bindings and feeds the command palette. Its notation:
// `secondary` is Cmd on macOS and Ctrl elsewhere. An action does nothing until a screen handles it.

import { type Feature, hidden } from "../hidden";

export interface Shortcut {
  keys: string[];
  context: string;
  /** Single keys, only outside a text field. */
  navKeys: string[];
  label: string;
  section: "General" | "Navigation" | "Project" | "Manager";
  inPalette: boolean;
  action: string;
  feature?: Feature;
}

export const SHORTCUTS: Shortcut[] = [
  {
    keys: ["secondary-k"],
    context: "Aurelius",
    navKeys: [],
    label: "Search and commands",
    section: "General",
    inPalette: false,
    action: "OpenPalette",
  },
  {
    keys: ["secondary-p"],
    context: "Aurelius",
    navKeys: [],
    label: "Switch workspace",
    section: "General",
    inPalette: true,
    action: "SwitchWorkspace",
  },
  {
    keys: ["secondary-1"],
    context: "Aurelius",
    navKeys: [],
    label: "Go to New session",
    section: "General",
    inPalette: true,
    action: "GoDashboard",
  },
  {
    keys: ["secondary-5"],
    context: "Aurelius",
    navKeys: [],
    label: "Go to Board",
    section: "General",
    inPalette: true,
    action: "GoBoard",
  },
  {
    keys: ["secondary-3"],
    context: "Aurelius",
    navKeys: [],
    label: "Go to Employees",
    section: "General",
    inPalette: true,
    action: "GoEmployees",
  },
  {
    keys: ["secondary-4"],
    context: "Aurelius",
    navKeys: [],
    label: "Go to Org chart (Employees tab)",
    section: "General",
    inPalette: true,
    action: "GoOrgChart",
  },
  {
    keys: ["secondary-6"],
    context: "Aurelius",
    navKeys: [],
    label: "Go to Projects",
    section: "General",
    inPalette: true,
    action: "GoProjects",
  },
  {
    keys: ["secondary-shift-v"],
    context: "Aurelius",
    navKeys: [],
    label: "Go to Vault (items of the company and its projects)",
    section: "General",
    inPalette: true,
    action: "GoVault",
    feature: "Vault",
  },
  {
    keys: [],
    context: "Aurelius",
    navKeys: [],
    label: "Go to Guidelines (Knowledge tab)",
    section: "General",
    inPalette: true,
    action: "GoGuidelines",
  },
  {
    keys: [],
    context: "Aurelius",
    navKeys: [],
    label: "Find guideline\u2026",
    section: "General",
    inPalette: true,
    action: "FindGuideline",
  },
  {
    keys: [],
    context: "Aurelius",
    navKeys: [],
    label: "New guideline",
    section: "General",
    inPalette: true,
    action: "NewGuideline",
  },
  {
    keys: ["secondary-n"],
    context: "Aurelius",
    navKeys: ["n"],
    label: "New (new session, project, task or memory, depending on the page)",
    section: "General",
    inPalette: true,
    action: "NewItem",
  },
  {
    keys: ["alt-j"],
    context: "Aurelius",
    navKeys: [],
    label: "Give work: choose the project",
    section: "General",
    inPalette: true,
    action: "CycleGiveProject",
  },
  {
    keys: ["secondary-shift-n"],
    context: "Aurelius",
    navKeys: [],
    label: "New project",
    section: "General",
    inPalette: true,
    action: "NewProject",
  },
  {
    keys: ["secondary-l"],
    context: "Aurelius",
    navKeys: ["/"],
    label: "Focus the input field",
    section: "General",
    inPalette: true,
    action: "FocusComposer",
  },
  {
    keys: ["escape"],
    context: "Aurelius",
    navKeys: ["backspace"],
    label: "Close / one level up",
    section: "General",
    inPalette: false,
    action: "Back",
  },
  {
    keys: ["alt-left"],
    context: "Aurelius",
    navKeys: [],
    label: "Back (previous screen)",
    section: "General",
    inPalette: true,
    action: "NavBack",
  },
  {
    keys: ["alt-right"],
    context: "Aurelius",
    navKeys: [],
    label: "Forward",
    section: "General",
    inPalette: true,
    action: "NavForward",
  },
  {
    keys: ["secondary-."],
    context: "Aurelius",
    navKeys: [],
    label: "Stop answer",
    section: "General",
    inPalette: true,
    action: "StopGeneration",
  },
  {
    keys: ["alt-enter"],
    context: "Aurelius",
    navKeys: [],
    label:
      "Send now: interrupt the running answer with your message (Enter does the same while an answer runs; Shift+Enter queues it)",
    section: "General",
    inPalette: true,
    action: "SendNow",
  },
  {
    keys: ["secondary-o"],
    context: "Aurelius",
    navKeys: [],
    label: "Add files",
    section: "General",
    inPalette: false,
    action: "AttachFiles",
  },
  {
    keys: ["secondary-r"],
    context: "Aurelius",
    navKeys: [],
    label: "Retry last answer (new version)",
    section: "General",
    inPalette: true,
    action: "RetryAnswer",
  },
  {
    keys: ["secondary-i", "secondary-2"],
    context: "Aurelius",
    navKeys: ["i"],
    label: "Notifications: what waits for you and what happened",
    section: "General",
    inPalette: true,
    action: "OpenNotifications",
  },
  {
    keys: ["secondary-m"],
    context: "Aurelius",
    navKeys: [],
    label: "Model and effort for this chat or task",
    section: "General",
    inPalette: true,
    action: "CycleModel",
  },
  {
    keys: ["secondary-u"],
    context: "Aurelius",
    navKeys: ["u"],
    label: "Show usage (subscription, tokens and costs)",
    section: "General",
    inPalette: true,
    action: "OpenUsage",
  },
  {
    keys: ["shift-tab"],
    context: "Aurelius",
    navKeys: [],
    label: "Switch this agent's mode (Ask \u2192 Accept edits \u2192 Auto \u2192 Plan \u2192 Ask)",
    section: "General",
    inPalette: true,
    action: "CycleAgentMode",
  },
  {
    keys: ["alt-b"],
    context: "Aurelius",
    navKeys: [],
    label: "Browser on / off for this employee (your own Chrome)",
    section: "Manager",
    inPalette: true,
    action: "ToggleBrowser",
  },
  {
    keys: ["alt-s"],
    context: "Aurelius",
    navKeys: [],
    label: "Screen on / off: screenshots without asking each time",
    section: "Manager",
    inPalette: true,
    action: "ToggleScreen",
  },
  {
    keys: ["secondary-shift-u"],
    context: "Aurelius",
    navKeys: [],
    label: "Updates: check and install",
    section: "General",
    inPalette: true,
    action: "OpenUpdates",
  },
  {
    keys: ["secondary-shift-k"],
    context: "Aurelius",
    navKeys: [],
    label: "Settings: Backups",
    section: "General",
    inPalette: true,
    action: "OpenBackups",
  },
  {
    keys: ["secondary-,", "secondary-9"],
    context: "Aurelius",
    navKeys: [],
    label: "Settings",
    section: "General",
    inPalette: true,
    action: "OpenSettings",
  },
  {
    keys: ["secondary-alt-r"],
    context: "Aurelius",
    navKeys: [],
    label: "Remote access: pair a phone or computer, devices, notifications",
    section: "General",
    inPalette: true,
    action: "OpenRemoteAccess",
  },
  {
    keys: ["secondary-alt-s"],
    context: "Aurelius",
    navKeys: [],
    label: "Servers: which Aurelius server this app uses, pair one",
    section: "General",
    inPalette: true,
    action: "OpenServers",
  },
  {
    keys: ["secondary-q"],
    context: "Aurelius",
    navKeys: [],
    label: "Close window (background work continues)",
    section: "General",
    inPalette: true,
    action: "Quit",
  },
  {
    keys: ["down"],
    context: "Workspace",
    navKeys: ["j"],
    label: "Next item",
    section: "Navigation",
    inPalette: false,
    action: "SelectNext",
  },
  {
    keys: ["up"],
    context: "Workspace",
    navKeys: ["k"],
    label: "Previous item",
    section: "Navigation",
    inPalette: false,
    action: "SelectPrev",
  },
  {
    keys: ["enter"],
    context: "Workspace",
    navKeys: ["o"],
    label: "Open",
    section: "Navigation",
    inPalette: false,
    action: "OpenSelected",
  },
  {
    keys: ["pageup"],
    context: "Workspace",
    navKeys: [],
    label: "Scroll page up",
    section: "Navigation",
    inPalette: false,
    action: "ScrollPageUp",
  },
  {
    keys: ["pagedown"],
    context: "Workspace",
    navKeys: ["space"],
    label: "Scroll page down",
    section: "Navigation",
    inPalette: false,
    action: "ScrollPageDown",
  },
  {
    keys: ["secondary-home"],
    context: "Workspace",
    navKeys: ["home"],
    label: "Go to top",
    section: "Navigation",
    inPalette: false,
    action: "ScrollTop",
  },
  {
    keys: ["secondary-end"],
    context: "Workspace",
    navKeys: ["end"],
    label: "Go to bottom",
    section: "Navigation",
    inPalette: false,
    action: "ScrollBottom",
  },
  {
    keys: ["1"],
    context: "Workspace",
    navKeys: [],
    label: "Filter 1 (Active / All)",
    section: "Navigation",
    inPalette: false,
    action: "Filter1",
  },
  {
    keys: ["3"],
    context: "Workspace",
    navKeys: [],
    label: "Filter 3 (Completed)",
    section: "Navigation",
    inPalette: false,
    action: "Filter3",
  },
  {
    keys: ["secondary-tab"],
    context: "Aurelius",
    navKeys: ["l", "right"],
    label: "Next tab",
    section: "Project",
    inPalette: false,
    action: "NextTab",
  },
  {
    keys: ["secondary-shift-tab"],
    context: "Aurelius",
    navKeys: ["h", "left"],
    label: "Previous tab",
    section: "Project",
    inPalette: false,
    action: "PrevTab",
  },
  {
    keys: ["secondary-shift-o"],
    context: "Aurelius",
    navKeys: [],
    label: "Open the project folder",
    section: "Project",
    inPalette: true,
    action: "OpenProjectFolder",
  },
  {
    keys: ["secondary-shift-a"],
    context: "Aurelius",
    navKeys: [],
    label: "Export the project (Markdown + JSON, into its folder)",
    section: "Project",
    inPalette: true,
    action: "ExportProject",
  },
  {
    keys: ["secondary-shift-g"],
    context: "Aurelius",
    navKeys: [],
    label: "Choose another folder (the open employee's workspace, or the open project)",
    section: "Project",
    inPalette: true,
    action: "ChangeProjectFolder",
  },
  {
    keys: ["f2"],
    context: "Aurelius",
    navKeys: [],
    label: "Edit project or employee",
    section: "Project",
    inPalette: true,
    action: "EditItem",
  },
  {
    keys: ["secondary-shift-m"],
    context: "Aurelius",
    navKeys: [],
    label: "Add employee (opens Employees)",
    section: "Manager",
    inPalette: true,
    action: "AddEmployee",
  },
  {
    keys: [],
    context: "Aurelius",
    navKeys: [],
    label: "Memory\u2026 (choose a project)",
    section: "Project",
    inPalette: true,
    action: "OpenMemory",
  },
  {
    keys: ["secondary-shift-,"],
    context: "Aurelius",
    navKeys: [],
    label: "Edit project: name, description, folder",
    section: "Project",
    inPalette: true,
    action: "OpenProjectSettings",
  },
  {
    keys: ["secondary-t"],
    context: "Aurelius",
    navKeys: [],
    label: "New ticket",
    section: "Project",
    inPalette: true,
    action: "MakeTicket",
  },
  {
    keys: ["secondary-shift-p"],
    context: "Aurelius",
    navKeys: ["p"],
    label: "Pause / resume (employee or project)",
    section: "Project",
    inPalette: true,
    action: "TogglePause",
  },
  {
    keys: ["secondary-shift-backspace"],
    context: "Aurelius",
    navKeys: [],
    label: "Archive (with confirmation)",
    section: "Project",
    inPalette: true,
    action: "ArchiveItem",
  },
  {
    keys: ["alt-shift-p"],
    context: "Aurelius",
    navKeys: [],
    label: "Open / close the latest plan",
    section: "General",
    inPalette: true,
    action: "TogglePlanFold",
  },
  {
    keys: ["delete"],
    context: "Workspace",
    navKeys: [],
    label: "Forget memory (Memory)",
    section: "Project",
    inPalette: false,
    action: "RemoveMemory",
  },
  {
    keys: ["secondary-shift-b"],
    context: "Aurelius",
    navKeys: ["b"],
    label: "Pin / unpin memory",
    section: "Project",
    inPalette: true,
    action: "TogglePin",
  },
  {
    keys: ["secondary-shift-e"],
    context: "Aurelius",
    navKeys: ["e"],
    label: "Show the context of the last answer",
    section: "General",
    inPalette: true,
    action: "ShowContext",
  },
  {
    keys: ["secondary-enter"],
    context: "Aurelius",
    navKeys: [],
    label: "Confirm (dialog)",
    section: "Project",
    inPalette: false,
    action: "ConfirmPrimary",
  },
];

/** Bindings of one context, beside the table (`overlay_bindings`). */
export const CONTEXT_BINDINGS: { key: string; action: string; context: string }[] = [
  { key: "1", action: "QuestionOption1", context: "Decision" },
  { key: "2", action: "QuestionOption2", context: "Decision" },
  { key: "3", action: "QuestionOption3", context: "Decision" },
  { key: "4", action: "QuestionOption4", context: "Decision" },
  { key: "tab", action: "QuestionNext", context: "Decision" },
  { key: "t", action: "QuestionOwnAnswer", context: "Decision" },
  { key: "x", action: "RejectApproval", context: "Decision" },
  { key: "secondary-backspace", action: "RejectApproval", context: "Decision" },
  { key: "secondary-alt-m", action: "CommentOnSelection", context: "PlanPanel" },
  { key: "up", action: "SelectPrev", context: "Settings" },
  { key: "k", action: "SelectPrev", context: "Settings" },
  { key: "down", action: "SelectNext", context: "Settings" },
  { key: "j", action: "SelectNext", context: "Settings" },
  { key: "v", action: "SettingsVerify", context: "Settings" },
  { key: "d", action: "SettingsUse", context: "Settings" },
  { key: "enter", action: "SettingsUse", context: "Settings" },
  { key: "secondary-enter", action: "SettingsUse", context: "Settings" },
  { key: "s", action: "SettingsKey", context: "Settings" },
  { key: "n", action: "SettingsNew", context: "Settings" },
  { key: "n", action: "MakeTicket", context: "Board" },
  { key: "secondary-enter", action: "DecideYes", context: "Decision" },
  { key: "delete", action: "SettingsRemove", context: "Settings" },
  { key: "c", action: "SettingsCheckUpdate", context: "Settings" },
  { key: "u", action: "SettingsInstallUpdate", context: "Settings" },
  { key: "i", action: "SettingsInstallAppUpdate", context: "Settings" },
  { key: "e", action: "SettingsRestartServer", context: "Settings" },
  { key: "q", action: "SettingsShutdownNow", context: "Settings" },
  { key: "g", action: "SettingsShutdownIdle", context: "Settings" },
  { key: "b", action: "SettingsBackupNow", context: "Settings" },
  { key: "h", action: "SettingsShowHints", context: "Settings" },
  { key: "w", action: "SettingsAppearance", context: "Settings" },
  { key: "l", action: "SettingsLoginStart", context: "Settings" },
  { key: "r", action: "SettingsRestore", context: "Settings" },
  { key: "p", action: "SettingsResume", context: "Settings" },
  { key: "o", action: "SettingsRemoteToggle", context: "Settings" },
  { key: "x", action: "SettingsRefusePairing", context: "Settings" },
  { key: "secondary-backspace", action: "SettingsRefusePairing", context: "Settings" },
  { key: "a", action: "SettingsPushKey", context: "Settings" },
  { key: "t", action: "SettingsPushTest", context: "Settings" },
  { key: "y", action: "SettingsCopyPairing", context: "Settings" },
  { key: "f2", action: "SettingsRenameServer", context: "Settings" },
  { key: "secondary-s", action: "LoadSave", context: "Settings" },
  { key: "alt-e", action: "LoadToggleEnabled", context: "Settings" },
  { key: "1", action: "Provider1", context: "AddConnection" },
  { key: "2", action: "Provider2", context: "AddConnection" },
  { key: "3", action: "Provider3", context: "AddConnection" },
  { key: "secondary-o", action: "ChooseDialogFolder", context: "AureliusDialog" },
  { key: "alt-e", action: "CycleTaskEffort", context: "AureliusDialog" },
  { key: "alt-e", action: "CycleTaskEffort", context: "GiveWork" },
  { key: "alt-p", action: "CycleReportsTo", context: "AureliusDialog" },
  { key: "alt-u", action: "CycleSubagentModel", context: "AureliusDialog" },
  { key: "down", action: "FolderNext", context: "ServerFolder" },
  { key: "up", action: "FolderPrev", context: "ServerFolder" },
  { key: "enter", action: "FolderOpen", context: "ServerFolder" },
  { key: "right", action: "FolderOpen", context: "ServerFolder" },
  { key: "left", action: "FolderParent", context: "ServerFolder" },
  { key: "backspace", action: "FolderParent", context: "ServerFolder" },
  { key: "secondary-enter", action: "FolderUse", context: "ServerFolder" },
  { key: "secondary-shift-n", action: "FolderNewFolder", context: "ServerFolder" },
  { key: "enter", action: "AllowOnce", context: "Inbox" },
  { key: "shift-enter", action: "AlwaysAllow", context: "Inbox" },
  { key: "d", action: "DenyApproval", context: "Inbox" },
  { key: "s", action: "BoardStart", context: "Board" },
  { key: "m", action: "BoardMoveTo", context: "Board" },
  { key: "a", action: "BoardResponsible", context: "Board" },
  { key: "f", action: "BoardFollow", context: "Board" },
  { key: "c", action: "BoardCopyLink", context: "Board" },
  { key: "alt-m", action: "CycleTaskManager", context: "GiveWork" },
  { key: "alt-o", action: "CycleTaskMode", context: "GiveWork" },
  { key: "alt-l", action: "CycleTaskModel", context: "GiveWork" },
  { key: "alt-j", action: "CycleGiveProject", context: "GiveWork" },
  { key: "secondary-enter", action: "SubmitGiveWork", context: "GiveWork" },
  { key: "secondary-enter", action: "ConfirmPrimary", context: "AureliusDialog" },
  { key: "secondary-enter", action: "StartNewSession", context: "NewSession" },
  { key: "alt-e", action: "CycleTaskEffort", context: "NewSession" },
  { key: "alt-m", action: "CycleTaskManager", context: "NewSession" },
  { key: "alt-o", action: "CycleTaskMode", context: "NewSession" },
  { key: "alt-l", action: "CycleTaskModel", context: "NewSession" },
];

/** A browser keeps these for itself; on the web the action gets another key. */
const BROWSER_RESERVED: Record<string, string> = {
  "secondary-t": "alt-t",
  "secondary-n": "alt-n",
  "secondary-shift-n": "alt-shift-n",
  "secondary-tab": "alt-]",
  "secondary-shift-tab": "alt-[",
  "secondary-w": "alt-w",
  "secondary-q": "",
};

export function keysFor(keys: string[], kind: "electron" | "web"): string[] {
  if (kind !== "web") return keys;
  return keys.map((k) => BROWSER_RESERVED[k] ?? k).filter(Boolean);
}

export interface KeyEventLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

const NAMED: Record<string, string> = {
  arrowup: "up",
  arrowdown: "down",
  arrowleft: "left",
  arrowright: "right",
  " ": "space",
  esc: "escape",
};

const BY_CODE: Record<string, string> = { Comma: ",", Period: ".", BracketLeft: "[", BracketRight: "]", Slash: "/" };

/** The chord of a key press, in the table's notation; null for a modifier alone. */
export function chordOf(event: KeyEventLike, mac: boolean): string | null {
  let key = event.key.toLowerCase();
  if (key === "control" || key === "shift" || key === "alt" || key === "meta") return null;
  key = NAMED[key] ?? key;
  // Letters follow the layout, so an AZERTY M is an M. Digits and punctuation may need Shift or
  // AltGr on one, so those go by their place.
  if (key.length === 1 && !/[a-z]/.test(key)) {
    const digit = /^(?:Digit|Numpad)(\d)$/.exec(event.code)?.[1];
    key = digit ?? BY_CODE[event.code] ?? key;
  }
  const secondary = mac ? event.metaKey : event.ctrlKey;
  return [secondary ? "secondary" : null, event.altKey ? "alt" : null, event.shiftKey ? "shift" : null, key]
    .filter(Boolean)
    .join("-");
}

export interface KeyState {
  /** The open contexts, the innermost last. */
  stack: { name: string; modal: boolean }[];
  /** The focus is in a text field. */
  typing: boolean;
  /** The focus is on a button or link: Enter and Space are its own. */
  onControl: boolean;
  kind: "electron" | "web";
}

/** A chord that means something in a text field stays out of the shortcuts, unless it has a modifier. */
// Ctrl+Backspace deletes a word in a text field; it must never reject a plan or a pairing there.
const TEXT_EDIT_CHORDS = new Set(["secondary-backspace"]);

function safeWhileTyping(chord: string): boolean {
  if (TEXT_EDIT_CHORDS.has(chord)) return false;
  return (
    chord.startsWith("secondary") ||
    chord.startsWith("alt") ||
    chord === "escape" ||
    chord === "shift-tab" ||
    /^f\d+$/.test(chord)
  );
}

/** The actions a chord may mean here, the innermost first. The first that has a handler wins. */
export function actionsFor(chord: string, state: KeyState): string[] {
  if (state.onControl && (chord === "enter" || chord === "space")) return [];
  if (state.typing && !safeWhileTyping(chord)) return [];
  const names: string[] = [];
  for (let i = state.stack.length - 1; i >= 0; i--) {
    const context = state.stack[i];
    if (context === undefined) continue;
    names.push(context.name);
    if (context.modal) break;
  }
  const out: string[] = [];
  for (const name of names) {
    for (const b of CONTEXT_BINDINGS) if (b.context === name && b.key === chord) out.push(b.action);
    for (const s of SHORTCUTS) {
      if (s.context !== name || (s.feature && hidden(s.feature))) continue;
      if (keysFor(s.keys, state.kind).includes(chord) || (!state.typing && s.navKeys.includes(chord)))
        out.push(s.action);
    }
  }
  return out;
}

const SYMBOLS: Record<string, string> = {
  enter: "↵",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  escape: "Esc",
  backspace: "⌫",
  tab: "Tab",
  space: "Space",
};

/** "Ctrl K", "⌘⇧N": how a chord is shown. */
export function showChord(chord: string, mac: boolean): string {
  const parts = chord.split("-").map((part) => {
    if (part === "secondary") return mac ? "⌘" : "Ctrl";
    if (part === "alt") return mac ? "⌥" : "Alt";
    if (part === "shift") return "⇧";
    return SYMBOLS[part] ?? part.toUpperCase();
  });
  return parts.join(mac ? "" : " ");
}
