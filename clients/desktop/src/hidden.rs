//! Features the desktop app keeps out of its menus, tabs, buttons, shortcuts and the command
//! palette (ticket #19): the measured usage was zero. Nothing is removed: the code, the
//! server's data and the protocol stay, so showing a feature again is deleting its line from
//! `HIDDEN_FEATURES`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    /// No desktop screen has them; listed so one list names everything hidden on every client.
    Notes,
    Shares,
    TaskVariants,
    TaskDependencies,
    /// Claude artifacts: the Settings toggle and claude.ai sign-in, the View / Open / Copy link buttons in a chat.
    Artifacts,
    /// An employee's heartbeat: the Alt+H shortcut and its menu entry.
    Heartbeats,
    /// Settings → Advanced → App keys.
    AppKeys,
    /// Keys of outside apps and of agent functions: no desktop screen of their own besides App keys.
    ApiKeys,
    /// Settings → Advanced → Plugins, and the plugin sections of Edit employee.
    Plugins,
    /// "+ File" beside a chat's composer and dropping files on a chat. Attachments on tickets,
    /// tasks and ideas, and pasted images in New ticket, stay.
    ChatAttachments,
}

/// The one list. Webhooks, agent functions, the vault and recurring tasks are not in it.
pub const HIDDEN_FEATURES: &[Feature] = &[
    Feature::Notes,
    Feature::Shares,
    Feature::TaskVariants,
    Feature::TaskDependencies,
    Feature::Artifacts,
    Feature::Heartbeats,
    Feature::AppKeys,
    Feature::ApiKeys,
    Feature::Plugins,
    Feature::ChatAttachments,
];

pub const fn hidden(feature: Feature) -> bool {
    let mut i = 0;
    while i < HIDDEN_FEATURES.len() {
        if HIDDEN_FEATURES[i] as u8 == feature as u8 {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_listed_features_are_hidden() {
        for f in HIDDEN_FEATURES {
            assert!(hidden(*f));
        }
        assert!(hidden(Feature::Plugins) && hidden(Feature::ChatAttachments));
    }
}
