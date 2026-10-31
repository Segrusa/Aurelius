//! Every action in Aurelius has a keyboard shortcut. This table is the single
//! source of truth: it registers the bindings and feeds the command palette (Ctrl+K).
//!
//! `secondary` is Cmd on macOS and Ctrl on Windows.

use gpui_kit::*;

actions!(
    aurelius,
    [
        OpenPalette,
        GoDashboard,
        GoTeamChat,
        GoBoard,
        GoEmployees,
        GoOrgChart,
        GoProjects,
        GoTasks,
        GoIdeas,
        CycleGiveProject,
        SubmitGiveWork,
        NewItem,
        NewProject,
        Back,
        InfoToggle,
        InfoClose,
        FocusComposer,
        TogglePreview,
        Quit,
        SelectNext,
        SelectPrev,
        OpenSelected,
        NextTab,
        PrevTab,
        Filter1,
        Filter2,
        Filter3,
        Filter4,
        ToggleHistory,
        ReviewApproval,
        ConfirmPrimary,
        PaletteUp,
        PaletteDown,
        PaletteConfirm,
        PaletteClose,
        DialogNextField,
        DialogCancel,
        StopGeneration,
        SendNow,
        RetryAnswer,
        AddEmployee,
        OpenMemory,
        EditItem,
        TogglePause,
        ArchiveItem,
        OpenInbox,
        AcceptMemory,
        CycleAgentMode,
        CycleHeartbeat,
        ToggleBrowser,
        ToggleScreen,
        OpenUsage,
        PanelConfirm,
        OpenProjectFolder,
        ChooseDialogFolder,
        SettingsCheckUpdate,
        SettingsBackupNow,
        NavBack,
        NavForward,
        ToggleSidebar,
        SettingsShowHints,
        SettingsArtifacts,
        SettingsTrace,
        SettingsAppearance,
        MenuNext,
        MenuPrev,
        MenuRun,
        SettingsLoginStart,
        SettingsRestore,
        SettingsResume,
        OpenBackups,
        ExportProject,
        SettingsInstallUpdate,
        SettingsInstallAppUpdate,
        SettingsRestartServer,
        SettingsShutdownNow,
        SettingsShutdownIdle,
        OpenUpdates,
        ChangeProjectFolder,
        OpenProjectSettings,
        TogglePlanFold,
        RemoveMemory,
        TogglePin,
        ShowContext,
        InboxUp,
        InboxDown,
        RejectApproval,
        OpenSettings,
        SettingsVerify,
        SettingsUse,
        SettingsKey,
        SettingsNew,
        SettingsRemove,
        SettingsModel,
        CycleModel,
        AssignSelectedTask,
        OpenTaskConversation,
        CycleTaskManager,
        CycleTaskMode,
        CycleTaskModel,
        CycleTaskEffort,
        ToggleManagerSequential,
        ToggleManagementMode,
        ToggleAutoAllow,
        CycleReportsTo,
        CycleSubagentModel,
        NewTask,
        AttachFiles,
        SettingsNextTab,
        CancelSelectedTask,
        QuestionOption1,
        QuestionOption2,
        QuestionOption3,
        QuestionOption4,
        QuestionNext,
        QuestionOwnAnswer,
        ScrollPageUp,
        ScrollPageDown,
        ScrollTop,
        ScrollBottom,
        Provider1,
        Provider2,
        Provider3,
        OpenRemoteAccess,
        SettingsRemoteToggle,
        SettingsRefusePairing,
        SettingsPushKey,
        SettingsPushTest,
        SettingsCopyPairing,
        OpenServers,
        FolderNext,
        FolderPrev,
        FolderOpen,
        FolderParent,
        FolderUse,
        FolderNewFolder,
        FolderErase,
        FolderTypePath,
        FolderSystemDialog,
        LoadNextField,
        LoadPrevField,
        LoadLeaveField,
        LoadSave,
        LoadToggleEnabled,
        BoardLeft,
        BoardRight,
        BoardUp,
        BoardDown,
        BoardOpen,
        BoardStart,
        BoardMoveTo,
        BoardPickUp,
    ]
);

/// Key contexts, outermost first.
pub const APP: &str = "Aurelius";
/// Main content has focus (not a text field): single-letter keys are active.
pub const NAV: &str = "Workspace";
pub const PALETTE: &str = "Palette";
pub const DIALOG: &str = "AureliusDialog";
/// The Dashboard's Give work box.
pub const GIVE_WORK: &str = "GiveWork";
pub const INBOX: &str = "Inbox";
/// A side panel (screenshot, markdown, artifact).
pub const SIDE_PANEL: &str = "SidePanel";
pub const SETTINGS: &str = "Settings";
/// Settings → Load limits: its own context, so typing in its fields never
/// triggers the single-letter Settings keys.
pub const LOAD: &str = "LoadLimits";
/// A plan Claude handed in, reviewed in the right-hand drawer.
pub const PLAN_REVIEW: &str = "PlanReview";
pub const MENU: &str = "ItemMenu";
/// A focused info icon: Enter and Space open its popover, Esc closes it.
pub const INFO: &str = "InfoIcon";
/// The folder browser (on this computer or the server's).
pub const FOLDERS: &str = "ServerFolder";
/// The Board page (inside `NAV`): its cards move with the keyboard (ui-ontwerp §15).
pub const BOARD: &str = "Board";
/// The Board's keys, never while a text field (Find) has focus.
const BOARD_KEYS: &str = "Board && !Input";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Navigation,
    Project,
    Manager,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Navigation => "List navigation",
            Section::Project => "Project",
            Section::Manager => "Employee",
        }
    }
}

pub struct Shortcut {
    /// GPUI keystroke syntax, bound in `context`; the first one is shown in the UI.
    pub keys: &'static [&'static str],
    pub context: Option<&'static str>,
    /// Single-key alternatives, only active while no text field has focus.
    pub nav_keys: &'static [&'static str],
    pub label: &'static str,
    pub section: Section,
    /// Listed as a command in the palette.
    pub in_palette: bool,
    pub action: fn() -> Box<dyn Action>,
}

macro_rules! sc {
    ($keys:expr, $ctx:expr, $nav:expr, $label:expr, $section:ident, $palette:expr, $action:ident) => {
        Shortcut {
            keys: $keys,
            context: $ctx,
            nav_keys: $nav,
            label: $label,
            section: Section::$section,
            in_palette: $palette,
            action: || Box::new($action),
        }
    };
}

const NONE: &[&str] = &[];

pub const SHORTCUTS: &[Shortcut] = &[
    // General: work everywhere, also while typing.
    sc!(&["secondary-k"], Some(APP), NONE, "Search and commands", General, false, OpenPalette),
    sc!(&["secondary-1"], Some(APP), NONE, "Go to Dashboard", General, true, GoDashboard),
    sc!(&["secondary-5"], Some(APP), NONE, "Go to Board (tickets of every project)", General, true, GoBoard),
    sc!(&["secondary-8"], Some(APP), NONE, "Go to Team chat (you and every employee)", General, true, GoTeamChat),
    sc!(&["secondary-3"], Some(APP), NONE, "Go to Employees", General, true, GoEmployees),
    sc!(&["secondary-4"], Some(APP), NONE, "Go to Org chart (Employees tab)", General, true, GoOrgChart),
    sc!(&["secondary-6"], Some(APP), NONE, "Go to Projects", General, true, GoProjects),
    sc!(&["secondary-7"], Some(APP), NONE, "Go to Tasks (all projects)", General, true, GoTasks),
    sc!(&["secondary-0"], Some(APP), NONE, "Go to Ideas (Projects tab)", General, true, GoIdeas),
    sc!(&["secondary-n"], Some(APP), &["n"], "New (give work, project, task, memory or idea, depending on the page)", General, true, NewItem),
    sc!(&["alt-j"], Some(APP), NONE, "Give work: choose the project", General, true, CycleGiveProject),
    sc!(&["secondary-shift-n"], Some(APP), NONE, "New project", General, true, NewProject),
    sc!(&["secondary-l"], Some(APP), &["/"], "Focus the input field", General, true, FocusComposer),
    sc!(&["escape"], Some(APP), &["backspace"], "Close / one level up", General, false, Back),
    sc!(&["alt-left"], Some(APP), NONE, "Back (previous screen)", General, true, NavBack),
    sc!(&["alt-right"], Some(APP), NONE, "Forward", General, true, NavForward),
    sc!(&["secondary-b"], Some(APP), NONE, "Expand / collapse the sidebar", General, true, ToggleSidebar),
    sc!(&["secondary-."], Some(APP), NONE, "Stop answer", General, true, StopGeneration),
    sc!(&["alt-enter"], Some(APP), NONE, "Send now: interrupt the running answer with your message (Enter does the same while an answer runs; Shift+Enter queues it)", General, true, SendNow),
    sc!(&["secondary-o"], Some(APP), NONE, "Add files (employee or project conversation)", General, true, AttachFiles),
    sc!(&["secondary-r"], Some(APP), NONE, "Retry last answer (new version)", General, true, RetryAnswer),
    sc!(&["secondary-i", "secondary-2"], Some(APP), &["i"], "Inbox: approvals and notifications", General, true, OpenInbox),
    sc!(&["secondary-m"], Some(APP), NONE, "Model and effort for this chat or task", General, true, CycleModel),
    sc!(&["secondary-u"], Some(APP), &["u"], "Show usage (subscription, tokens and costs)", General, true, OpenUsage),
    sc!(&["shift-tab"], Some(APP), NONE, "Switch this agent's mode (Ask → Accept edits → Auto → Plan → Ask)", General, true, CycleAgentMode),
    sc!(&["alt-h"], Some(APP), NONE, "Employee heartbeat: off → every 30 min → hourly → every 4 h", Manager, true, CycleHeartbeat),
    sc!(&["alt-b"], Some(APP), NONE, "Browser on / off for this employee (your own Chrome)", Manager, true, ToggleBrowser),
    sc!(&["alt-s"], Some(APP), NONE, "Screen on / off: screenshots without asking each time", Manager, true, ToggleScreen),
    sc!(&["secondary-shift-u"], Some(APP), NONE, "Updates: check and install", General, true, OpenUpdates),
    sc!(&["secondary-shift-k"], Some(APP), NONE, "Settings: Backups", General, true, OpenBackups),
    sc!(&["secondary-,", "secondary-9"], Some(APP), NONE, "Settings", General, true, OpenSettings),
    sc!(&["secondary-alt-r"], Some(APP), NONE, "Remote access: pair a phone or computer, devices, notifications", General, true, OpenRemoteAccess),
    sc!(&["secondary-alt-s"], Some(APP), NONE, "Servers: which Aurelius server this app uses, pair one", General, true, OpenServers),
    sc!(&["secondary-shift-d"], Some(APP), NONE, "Show / hide sample data", General, true, TogglePreview),
    sc!(&["secondary-q"], Some(APP), NONE, "Close window (background work continues)", General, true, Quit),
    // Lists: only while no text field has focus.
    sc!(&["down"], Some(NAV), &["j"], "Next item", Navigation, false, SelectNext),
    sc!(&["up"], Some(NAV), &["k"], "Previous item", Navigation, false, SelectPrev),
    sc!(&["enter"], Some(NAV), &["o"], "Open", Navigation, false, OpenSelected),
    sc!(&["pageup"], Some(NAV), NONE, "Scroll page up", Navigation, false, ScrollPageUp),
    sc!(&["pagedown"], Some(NAV), &["space"], "Scroll page down", Navigation, false, ScrollPageDown),
    sc!(&["secondary-home"], Some(NAV), &["home"], "Go to top", Navigation, false, ScrollTop),
    sc!(&["secondary-end"], Some(NAV), &["end"], "Go to bottom", Navigation, false, ScrollBottom),
    sc!(&["1"], Some(NAV), NONE, "Filter 1 (Active / All)", Navigation, false, Filter1),
    sc!(&["2"], Some(NAV), NONE, "Filter 2 (Waiting / Decisions)", Navigation, false, Filter2),
    sc!(&["3"], Some(NAV), NONE, "Filter 3 (Completed)", Navigation, false, Filter3),
    sc!(&["4"], Some(NAV), NONE, "Filter 4 (Ideas: whose)", Navigation, false, Filter4),
    // Project
    sc!(&["secondary-tab"], Some(APP), &["l", "right"], "Next tab", Project, false, NextTab),
    sc!(&["secondary-shift-tab"], Some(APP), &["h", "left"], "Previous tab", Project, false, PrevTab),
    sc!(&["secondary-shift-o"], Some(APP), NONE, "Open the project folder", Project, true, OpenProjectFolder),
    sc!(&["secondary-shift-a"], Some(APP), NONE, "Export the project (Markdown + JSON, into its folder)", Project, true, ExportProject),
    sc!(&["secondary-shift-g"], Some(APP), NONE, "Choose another folder (the open employee's workspace, or the open project)", Project, true, ChangeProjectFolder),
    sc!(&["f2"], Some(APP), NONE, "Edit project, employee or idea", Project, true, EditItem),
    sc!(&["secondary-shift-m"], Some(APP), NONE, "Add employee (opens Employees)", Manager, true, AddEmployee),
    sc!(&[], Some(APP), NONE, "Memory… (choose a project)", Project, true, OpenMemory),
    sc!(&["secondary-shift-,"], Some(APP), NONE, "Edit project: name, description, folder", Project, true, OpenProjectSettings),
    sc!(&["secondary-t"], Some(APP), NONE, "New task · in Ideas: start the selected idea as a task", Project, true, NewTask),
    sc!(&["secondary-shift-x"], Some(APP), &["x"], "Cancel selected task", Project, true, CancelSelectedTask),
    sc!(&["alt-a"], Some(APP), &["d"], "Give the selected task to an employee (or make it a draft)", Project, true, AssignSelectedTask),
    sc!(&["alt-c"], Some(APP), NONE, "Open the selected task's conversation with its employee", Project, true, OpenTaskConversation),
    sc!(&["secondary-shift-p"], Some(APP), &["p"], "Pause / resume (employee or project)", Project, true, TogglePause),
    sc!(&["secondary-shift-backspace"], Some(APP), NONE, "Archive (with confirmation)", Project, true, ArchiveItem),
    sc!(&["secondary-shift-r"], Some(APP), &["r"], "Review approval", Project, true, ReviewApproval),
    sc!(&["secondary-shift-y"], Some(APP), &["y"], "Accept memory proposal", Project, true, AcceptMemory),
    sc!(&["alt-shift-p"], Some(APP), NONE, "Fold / unfold the latest plan in this conversation", General, true, TogglePlanFold),
    sc!(&["delete"], Some(NAV), NONE, "Reject / forget memory (Memory) · drop idea (Ideas)", Project, false, RemoveMemory),
    sc!(&["secondary-shift-b"], Some(APP), &["b"], "Pin / unpin memory", Project, true, TogglePin),
    sc!(&["secondary-shift-e"], Some(APP), &["e"], "Show the context of the last answer", General, true, ShowContext),
    sc!(&["secondary-enter"], Some(APP), NONE, "Confirm (dialog)", Project, false, ConfirmPrimary),
    // Manager
    sc!(&["secondary-shift-h"], Some(APP), &["g"], "Show / hide task history", Manager, true, ToggleHistory),
];

/// Internal bindings for overlays; not listed in the overview.
fn overlay_bindings() -> Vec<KeyBinding> {
    let palette_input = "Palette > Input";
    let dialog_input = "AureliusDialog > Input";
    vec![
        key("up", PaletteUp, Some(palette_input)),
        key("down", PaletteDown, Some(palette_input)),
        key("ctrl-p", PaletteUp, Some(palette_input)),
        key("ctrl-n", PaletteDown, Some(palette_input)),
        key("enter", PaletteConfirm, Some(palette_input)),
        key("escape", PaletteClose, Some(palette_input)),
        key("tab", DialogNextField, Some(dialog_input)),
        key("secondary-o", ChooseDialogFolder, Some(DIALOG)),
        // New task dropdowns: which manager gets it (or none: a draft), and how its conversation runs
        // (unset: as the manager). ↑↓ + Enter pick.
        key("alt-m", CycleTaskManager, Some(DIALOG)),
        key("alt-o", CycleTaskMode, Some(DIALOG)),
        key("alt-l", CycleTaskModel, Some(DIALOG)),
        key("alt-e", CycleTaskEffort, Some(DIALOG)),
        key("alt-t", ToggleManagerSequential, Some(DIALOG)),
        key("alt-p", CycleReportsTo, Some(DIALOG)),
        // The same dropdowns in the Dashboard's Give work box; Ctrl+Enter gives the work.
        key("alt-m", CycleTaskManager, Some(GIVE_WORK)),
        key("alt-o", CycleTaskMode, Some(GIVE_WORK)),
        key("alt-l", CycleTaskModel, Some(GIVE_WORK)),
        key("alt-e", CycleTaskEffort, Some(GIVE_WORK)),
        key("secondary-enter", SubmitGiveWork, Some(GIVE_WORK)),
        // Model and effort of delegated tasks and of subagents (Edit manager, Settings → Connections).
        key("alt-u", CycleSubagentModel, Some(DIALOG)),
        key("alt-u", CycleSubagentModel, Some(SETTINGS)),
        key("up", InboxUp, Some(INBOX)),
        key("k", InboxUp, Some(INBOX)),
        key("down", InboxDown, Some(INBOX)),
        key("j", InboxDown, Some(INBOX)),
        // The inbox's filter tabs (Ctrl Tab works too).
        key("h", PrevTab, Some(INBOX)),
        key("l", NextTab, Some(INBOX)),
        key("secondary-backspace", RejectApproval, Some(INBOX)),
        key("x", RejectApproval, Some(INBOX)),
        key("escape", Back, Some(INBOX)),
        key("enter", PanelConfirm, Some(SIDE_PANEL)),
        key("escape", Back, Some(SIDE_PANEL)),
        // Answering Claude's questions in the inbox.
        key("1", QuestionOption1, Some(INBOX)),
        key("2", QuestionOption2, Some(INBOX)),
        key("3", QuestionOption3, Some(INBOX)),
        key("4", QuestionOption4, Some(INBOX)),
        key("tab", QuestionNext, Some(INBOX)),
        key("t", QuestionOwnAnswer, Some(INBOX)),
        key("enter", OpenSelected, Some(INBOX)),
        key("o", OpenSelected, Some(INBOX)),
        key("up", InboxUp, Some(SETTINGS)),
        key("k", InboxUp, Some(SETTINGS)),
        key("down", InboxDown, Some(SETTINGS)),
        key("j", InboxDown, Some(SETTINGS)),
        key("v", SettingsVerify, Some(SETTINGS)),
        key("d", SettingsUse, Some(SETTINGS)),
        key("enter", SettingsUse, Some(SETTINGS)),
        key("s", SettingsKey, Some(SETTINGS)),
        key("n", SettingsNew, Some(SETTINGS)),
        key("m", SettingsModel, Some(SETTINGS)),
        key("delete", SettingsRemove, Some(SETTINGS)),
        key("c", SettingsCheckUpdate, Some(SETTINGS)),
        key("u", SettingsInstallUpdate, Some(SETTINGS)),
        key("i", SettingsInstallAppUpdate, Some(SETTINGS)),
        key("e", SettingsRestartServer, Some(SETTINGS)),
        key("q", SettingsShutdownNow, Some(SETTINGS)),
        key("g", SettingsShutdownIdle, Some(SETTINGS)),
        key("b", SettingsBackupNow, Some(SETTINGS)),
        key("h", SettingsShowHints, Some(SETTINGS)),
        key("w", SettingsAppearance, Some(SETTINGS)),
        key("down", MenuNext, Some(MENU)),
        key("j", MenuNext, Some(MENU)),
        key("up", MenuPrev, Some(MENU)),
        key("k", MenuPrev, Some(MENU)),
        key("enter", MenuRun, Some(MENU)),
        key("enter", InfoToggle, Some(INFO)),
        key("space", InfoToggle, Some(INFO)),
        key("escape", InfoClose, Some(INFO)),
        key("escape", Back, Some(MENU)),
        key("l", SettingsLoginStart, Some(SETTINGS)),
        key("r", SettingsRestore, Some(SETTINGS)),
        key("p", SettingsResume, Some(SETTINGS)),
        key("o", SettingsRemoteToggle, Some(SETTINGS)),
        key("x", SettingsRefusePairing, Some(SETTINGS)),
        key("secondary-backspace", SettingsRefusePairing, Some(SETTINGS)),
        key("secondary-enter", SettingsUse, Some(SETTINGS)),
        key("a", SettingsPushKey, Some(SETTINGS)),
        key("t", SettingsPushTest, Some(SETTINGS)),
        key("y", SettingsCopyPairing, Some(SETTINGS)),
        // Folder browser: letters type into its filter, so no single-letter keys here.
        key("down", FolderNext, Some(FOLDERS)),
        key("up", FolderPrev, Some(FOLDERS)),
        key("enter", FolderOpen, Some(FOLDERS)),
        key("right", FolderOpen, Some(FOLDERS)),
        // Backspace shortens the filter first, then goes up.
        key("backspace", FolderErase, Some(FOLDERS)),
        key("left", FolderParent, Some(FOLDERS)),
        key("secondary-enter", FolderUse, Some(FOLDERS)),
        key("secondary-shift-n", FolderNewFolder, Some(FOLDERS)),
        key("secondary-l", FolderTypePath, Some(FOLDERS)),
        key("secondary-o", FolderSystemDialog, Some(FOLDERS)),
        key("escape", Back, Some(FOLDERS)),
        // Its path and new-folder fields: Esc closes the field, not the browser.
        key("escape", Back, Some("ServerFolder > Input")),
        key("escape", Back, Some(SETTINGS)),
        key("1", Provider1, Some(SETTINGS)),
        key("2", Provider2, Some(SETTINGS)),
        key("3", Provider3, Some(SETTINGS)),
        key("enter", ConfirmPrimary, Some("SettingsKey > Input")),
        key("escape", Back, Some("SettingsKey > Input")),
        key("enter", ConfirmPrimary, Some(DIALOG)),
        // Esc in the message composer returns to keyboard navigation.
        key("escape", Back, Some("Composer > Input")),
        key("escape", DialogCancel, Some(dialog_input)),
        key("secondary-enter", ConfirmPrimary, Some(dialog_input)),
        // Ctrl+K etc. must also work while a text field has focus.
        key("secondary-k", OpenPalette, Some("Input")),
        key("secondary-m", CycleModel, Some("Input")),
        key("secondary-o", AttachFiles, Some("Input")),
        key("secondary-o", FolderSystemDialog, Some("ServerFolder > Input")),
        key("alt-b", ToggleBrowser, Some("Input")),
        key("alt-s", ToggleScreen, Some("Input")),
        key("tab", SettingsNextTab, Some(SETTINGS)),
        key("tab", SettingsNextTab, Some(LOAD)),
        key("escape", Back, Some(LOAD)),
        key("enter", LoadNextField, Some(LOAD)),
        key("secondary-s", LoadSave, Some(LOAD)),
        key("alt-e", LoadToggleEnabled, Some(LOAD)),
        key("tab", LoadNextField, Some("LoadLimits > Input")),
        key("shift-tab", LoadPrevField, Some("LoadLimits > Input")),
        key("enter", LoadSave, Some("LoadLimits > Input")),
        key("escape", LoadLeaveField, Some("LoadLimits > Input")),
        key("escape", Back, Some(PLAN_REVIEW)),
        // The Board (§15): arrows between cards and columns, Enter opens, S starts,
        // M moves, Space picks a card up (then arrows, Enter drops, Esc cancels).
        key("left", BoardLeft, Some(BOARD_KEYS)),
        key("right", BoardRight, Some(BOARD_KEYS)),
        key("up", BoardUp, Some(BOARD_KEYS)),
        key("down", BoardDown, Some(BOARD_KEYS)),
        key("enter", BoardOpen, Some(BOARD_KEYS)),
        key("s", BoardStart, Some(BOARD_KEYS)),
        key("m", BoardMoveTo, Some(BOARD_KEYS)),
        key("space", BoardPickUp, Some(BOARD_KEYS)),
        key("secondary-1", GoDashboard, Some("Input")),
    ]
}

fn key<A: Action>(keys: &str, action: A, context: Option<&str>) -> KeyBinding {
    binding(keys, Box::new(action), context)
}

/// List keys, and keys that type text (a character, space, backspace), never fire while a
/// text field has focus, wherever it sits in the page (`!Input` looks at the whole stack).
fn typing_safe(keys: &str, context: &str) -> String {
    let types = keys.chars().count() == 1 || matches!(keys, "space" | "backspace");
    if context == NAV || (types && !context.contains("Input")) { format!("({context}) && !Input") } else { context.to_string() }
}

fn binding(keys: &str, action: Box<dyn Action>, context: Option<&str>) -> KeyBinding {
    let predicate = context.map(|c| KeyBindingContextPredicate::parse(&typing_safe(keys, c)).expect("valid key context").into());
    KeyBinding::load(keys, action, predicate, false, None, &DummyKeyboardMapper).expect("valid keystroke")
}

fn all_bindings() -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    for shortcut in SHORTCUTS {
        for key in shortcut.keys {
            bindings.push(binding(key, (shortcut.action)(), shortcut.context));
        }
        for key in shortcut.nav_keys {
            bindings.push(binding(key, (shortcut.action)(), Some(NAV)));
        }
    }
    bindings.extend(overlay_bindings());
    bindings
}

pub fn bind(cx: &mut App) {
    cx.bind_keys(all_bindings());
}

/// Human label for a keystroke, e.g. "secondary-shift-n" → "Ctrl ⇧ N" / "⌘ ⇧ N".
pub fn label(keys: &str) -> String {
    keys.split('-')
        .map(|part| match part {
            "secondary" => {
                if cfg!(target_os = "macos") { "⌘".to_string() } else { "Ctrl".to_string() }
            }
            "ctrl" => "Ctrl".into(),
            "alt" => {
                if cfg!(target_os = "macos") { "⌥".to_string() } else { "Alt".to_string() }
            }
            "shift" => "⇧".into(),
            "enter" => "Enter".into(),
            "escape" => "Esc".into(),
            "left" => "←".into(),
            "right" => "→".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            "space" => "Space".into(),
            "tab" => "Tab".into(),
            "pageup" => "PgUp".into(),
            "pagedown" => "PgDn".into(),
            "home" => "Home".into(),
            "end" => "End".into(),
            "backspace" => "⌫".into(),
            "delete" => "Del".into(),
            other if other.len() == 1 => other.to_uppercase(),
            other => other.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    // gpui_kit::* also exports a `test` macro, so name the std one.

    fn fires(keymap: &Keymap, key: &str, stack: &[&str]) -> bool {
        let stack: Vec<KeyContext> = stack.iter().map(|c| KeyContext::parse(c).unwrap()).collect();
        !keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &stack).0.is_empty()
    }

    #[::core::prelude::v1::test]
    fn single_keys_never_fire_while_a_text_field_has_focus() {
        let keymap = Keymap::new(all_bindings());
        // The Dashboard's Give work field sits inside the page (Workspace) and its own context.
        let typing = [APP, NAV, GIVE_WORK, "Input"];
        for key in ["i", "a", "n", "x", "d", "1", "j", "space", "backspace", "down", "delete"] {
            assert!(!fires(&keymap, key, &typing), "{key} fires while typing");
        }
        assert!(!fires(&keymap, "s", &[APP, SETTINGS, "Input"]), "Settings letters fire while typing");
        for key in ["h", "l"] {
            assert!(!fires(&keymap, key, &[APP, INBOX, "Input"]), "inbox tab key {key} fires while typing");
            assert!(fires(&keymap, key, &[APP, INBOX]));
        }
        // Without a text field they still work, and Ctrl shortcuts work while typing.
        assert!(fires(&keymap, "i", &[APP, NAV]));
        assert!(fires(&keymap, "s", &[APP, SETTINGS]));
        assert!(fires(&keymap, "secondary-i", &typing));
    }

    /// §15: on the Board the arrows, Enter, S, M and Space are the Board's, and none
    /// of them while its Find field has focus.
    #[::core::prelude::v1::test]
    fn the_board_keys_move_cards_but_never_while_typing() {
        let keymap = Keymap::new(all_bindings());
        let board = format!("{NAV} {BOARD}");
        let action = |key: &str, stack: &[&str]| {
            let stack: Vec<KeyContext> = stack.iter().map(|c| KeyContext::parse(c).unwrap()).collect();
            keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &stack).0.first().map(|b| b.action().name().to_string())
        };
        for (key, name) in [("left", "BoardLeft"), ("right", "BoardRight"), ("up", "BoardUp"), ("down", "BoardDown"), ("enter", "BoardOpen"), ("s", "BoardStart"), ("m", "BoardMoveTo"), ("space", "BoardPickUp")] {
            assert_eq!(action(key, &[APP, &board]).as_deref(), Some(format!("aurelius::{name}").as_str()), "{key}");
            assert!(!action(key, &[APP, &board, "Input"]).is_some_and(|a| a.starts_with("aurelius::Board")), "{key} fires while typing in Find");
            assert!(!action(key, &[APP, NAV]).is_some_and(|a| a.starts_with("aurelius::Board")), "{key} is the Board's only");
        }
        // Esc stays "one level up": it puts a held card back, then closes the panel.
        assert_eq!(action("escape", &[APP, &board]).as_deref(), Some("aurelius::Back"));
    }
}
