//! Design tokens from "Aurelius Core Screens" (claude.ai/design, dark mode).
//! Warm near-monochrome darks; the green accent is reserved for active state
//! and the primary action.
//!
//! The constants name the tokens by their dark value. In light mode `c` swaps
//! each token for its counterpart in `light`; other colours pass through.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::*;

pub const BG: u32 = 0x111210;
pub const SIDEBAR: u32 = 0x141512;
pub const PANEL: u32 = 0x151613;
pub const SURFACE: u32 = 0x181916;
pub const SURFACE_2: u32 = 0x1E1F1C;
pub const BORDER: u32 = 0x2C2D29;
pub const BORDER_STRONG: u32 = 0x3A3B36;
pub const TEXT: u32 = 0xECEBE5;
pub const TEXT_2: u32 = 0xA4A49C;
pub const MUTED: u32 = 0x71716B;
pub const ACCENT: u32 = 0x6F8F68;
pub const ACCENT_TEXT: u32 = 0x8FAE87;
pub const ACCENT_HALO: u32 = 0x1A2118;
pub const ATTENTION_BG: u32 = 0x161A15;
pub const ATTENTION_BORDER: u32 = 0x3B4A38;
pub const NEGATIVE: u32 = 0xB07A70;
pub const FOCUS_RING: u32 = 0x86A67E;
/// Marks what a manager wrote (the accent bar on its ideas).
pub const MANAGER_MARK: u32 = 0xB0956A;

/// Light counterparts of the tokens: warm paper with dark ink, the same green.
fn light(hex: u32) -> u32 {
    match hex {
        BG => 0xF7F6F2,
        SIDEBAR => 0xEFEEE9,
        PANEL => 0xF3F2EE,
        SURFACE => 0xFFFFFF,
        SURFACE_2 => 0xEAE9E3,
        BORDER => 0xDCDBD4,
        BORDER_STRONG => 0xC5C4BC,
        TEXT => 0x1C1D1A,
        TEXT_2 => 0x55564F,
        MUTED => 0x84847C,
        ACCENT => 0x557A4E,
        ACCENT_TEXT => 0x3E6837,
        ACCENT_HALO => 0xE2EBDE,
        ATTENTION_BG => 0xEEF3EB,
        ATTENTION_BORDER => 0xB7CCB1,
        NEGATIVE => 0xA4493C,
        FOCUS_RING => 0x6A8F62,
        MANAGER_MARK => 0x84683A,
        other => other,
    }
}

static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

pub fn c(hex: u32) -> Hsla {
    rgb(if is_light() { light(hex) } else { hex }).into()
}

/// The user's choice in Settings → App, kept in ui.json.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Global for Appearance {}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL.into_iter().find(|a| a.key() == key).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Appearance::System => Appearance::Light,
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::System,
        }
    }

    pub fn current(cx: &App) -> Self {
        cx.try_global::<Appearance>().copied().unwrap_or_default()
    }

    /// Light or dark, following Windows / macOS for `System`.
    fn light(self, cx: &App) -> bool {
        match self {
            Appearance::Light => true,
            Appearance::Dark => false,
            Appearance::System => matches!(cx.window_appearance(), WindowAppearance::Light | WindowAppearance::VibrantLight),
        }
    }
}

/// Font families resolved once at startup. Geist / Geist Mono / Newsreader are
/// used when installed; otherwise platform fallbacks.
#[derive(Clone)]
pub struct Fonts {
    pub ui: SharedString,
    pub mono: SharedString,
    pub serif: SharedString,
}

impl Global for Fonts {}

impl Fonts {
    pub fn resolve(cx: &App) -> Self {
        let installed = cx.text_system().all_font_names();
        let pick = |wanted: &[&str], fallback: &str| -> SharedString {
            wanted
                .iter()
                .find(|name| installed.iter().any(|f| f.eq_ignore_ascii_case(name)))
                .map(|s| SharedString::from(s.to_string()))
                .unwrap_or_else(|| fallback.to_string().into())
        };
        #[cfg(target_os = "windows")]
        let (ui, mono, serif) = ("Segoe UI", "Consolas", "Georgia");
        #[cfg(target_os = "macos")]
        let (ui, mono, serif) = (".SystemUIFont", "Menlo", "Georgia");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let (ui, mono, serif) = ("sans-serif", "monospace", "serif");
        Self {
            ui: pick(&["Geist"], ui),
            mono: pick(&["Geist Mono", "Cascadia Mono"], mono),
            serif: pick(&["Newsreader"], serif),
        }
    }

    pub fn get(cx: &App) -> &Self {
        cx.global::<Fonts>()
    }
}

/// Set the appearance and repaint (also when the system switches, for `System`).
pub fn set_appearance(appearance: Appearance, cx: &mut App) {
    cx.set_global(appearance);
    let light = appearance.light(cx);
    if light != is_light() {
        LIGHT.store(light, Ordering::Relaxed);
        colors(cx);
        cx.refresh_windows();
    }
}

/// Follow a system switch between light and dark.
pub fn system_changed(cx: &mut App) {
    set_appearance(Appearance::current(cx), cx);
}

pub fn apply(cx: &mut App) {
    cx.set_global(Fonts::resolve(cx));
    colors(cx);
}

/// Align gpui-kit's own components (inputs, dialogs, kbd tags) with the design.
fn colors(cx: &mut App) {
    let fonts = Fonts::get(cx).clone();
    let mode = if is_light() { gpui_kit::component::ThemeMode::Light } else { gpui_kit::component::ThemeMode::Dark };
    gpui_kit::component::Theme::change(mode, None, cx);
    gpui_kit::component::Theme::update(cx, |theme| {
        theme.font_family = fonts.ui.clone();
        theme.mono_font_family = fonts.mono.clone();
        theme.radius = px(6.);
        let colors = &mut theme.colors;
        colors.background = c(BG);
        colors.foreground = c(TEXT);
        colors.muted = c(SURFACE_2);
        colors.muted_foreground = c(MUTED);
        colors.border = c(BORDER);
        colors.input = c(BORDER);
        colors.ring = c(FOCUS_RING);
        colors.primary = c(ACCENT);
        colors.primary_hover = c(FOCUS_RING);
        colors.primary_active = c(ACCENT);
        colors.primary_foreground = c(BG);
        colors.secondary = c(SURFACE_2);
        colors.secondary_foreground = c(TEXT);
        colors.popover = c(SURFACE);
        colors.popover_foreground = c(TEXT);
        colors.accent = c(SURFACE_2);
        colors.accent_foreground = c(TEXT);
        colors.selection = Hsla { a: 0.35, ..c(ACCENT) };
        colors.caret = c(TEXT);
    });
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui_kit's own `test` macro would shadow the std one.
    use super::{
        light, Appearance, ACCENT, ACCENT_HALO, ACCENT_TEXT, ATTENTION_BG, ATTENTION_BORDER, BG, BORDER, BORDER_STRONG,
        FOCUS_RING, MANAGER_MARK, MUTED, NEGATIVE, PANEL, SIDEBAR, SURFACE, SURFACE_2, TEXT, TEXT_2,
    };

    #[test]
    fn every_token_has_its_own_light_colour() {
        let tokens = [
            BG, SIDEBAR, PANEL, SURFACE, SURFACE_2, BORDER, BORDER_STRONG, TEXT, TEXT_2, MUTED, ACCENT, ACCENT_TEXT,
            ACCENT_HALO, ATTENTION_BG, ATTENTION_BORDER, NEGATIVE, FOCUS_RING, MANAGER_MARK,
        ];
        let mut seen = std::collections::HashSet::new();
        for token in tokens {
            assert_ne!(light(token), token, "{token:06X} has no light colour");
            assert!(seen.insert(token), "{token:06X} is used by two tokens");
        }
        assert_eq!(light(0xD99A3A), 0xD99A3A, "other colours pass through");
    }

    #[test]
    fn the_appearance_choice_survives_ui_json() {
        for a in Appearance::ALL {
            assert_eq!(Appearance::from_key(a.key()), a);
        }
        assert_eq!(Appearance::from_key(""), Appearance::System);
    }
}
