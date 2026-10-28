//! A project in a window of its own, beside the main window: another
//! `AppView` held to that one project. It talks to the same server (its own
//! subscription), so both windows update live.

use crate::app::AppView;
use aurelius_domain::ProjectId;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;

/// The main window (closing it ends the UI, as before) and the open project windows.
#[derive(Default)]
pub struct Windows {
    pub main: Option<WindowId>,
    projects: HashMap<ProjectId, AnyWindowHandle>,
}

impl Global for Windows {}

/// Closing the main window quits, project windows included; closing a project window only closes it.
pub fn track_closes(cx: &mut App) {
    cx.on_window_closed(|cx, id| {
        let windows = cx.default_global::<Windows>();
        windows.projects.retain(|_, w| w.window_id() != id);
        let main_closed = windows.main == Some(id);
        if main_closed || cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

/// Opens `project` in its own window, or brings its window to the front.
pub fn open(project: ProjectId, name: &str, cx: &mut App) {
    if let Some(handle) = cx.default_global::<Windows>().projects.get(&project).copied() {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }
    let saved = &crate::app::ui_prefs()["project_windows"][project.to_string()];
    let mut display_id = None;
    let window_bounds = match serde_json::from_value::<Bounds<Pixels>>(saved["bounds"].clone()) {
        Ok(b) => {
            let displays = cx.displays();
            let areas: Vec<_> = displays.iter().map(|d| d.visible_bounds()).collect();
            let fallback = fallback_display(cx).and_then(|id| displays.iter().position(|d| d.id() == id));
            match fallback {
                Some(fallback) => {
                    let (b, on, own) = placement(b, &areas, fallback);
                    // GPUI's Windows backend drops bounds whose centre isn't on this display (primary if unset).
                    display_id = Some(displays[on].id());
                    match own && saved["maximized"].as_bool() == Some(true) {
                        true => WindowBounds::Maximized(b),
                        false => WindowBounds::Windowed(b),
                    }
                }
                None => WindowBounds::centered(b.size, cx),
            }
        }
        Err(_) => WindowBounds::centered(size(px(1180.), px(780.)), cx),
    };
    let options = WindowOptions {
        window_bounds: Some(window_bounds),
        display_id,
        window_min_size: Some(size(px(900.), px(640.))),
        titlebar: Some(TitlebarOptions {
            title: Some(title(name).into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(17.))),
            ..Default::default()
        }),
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| {
            let mut view = AppView::new(window, cx);
            view.hold_to_project(project, window, cx);
            view
        })
    });
    match opened {
        Ok((handle, _)) => {
            cx.default_global::<Windows>().projects.insert(project, handle);
        }
        Err(e) => tracing::warn!("could not open a window for project {project}: {e}"),
    }
}

/// The main window's display, else the primary display.
fn fallback_display(cx: &mut App) -> Option<DisplayId> {
    let main = cx.default_global::<Windows>().main;
    let on_main = cx
        .windows()
        .into_iter()
        .find(|w| Some(w.window_id()) == main)
        .and_then(|w| w.update(cx, |_, window, cx| window.display(cx)).ok().flatten());
    on_main.or_else(|| cx.primary_display()).map(|d| d.id())
}

/// Where a window saved at `saved` opens, given the usable area of each
/// display: the bounds, the index of the display they are on, and whether
/// that display is the saved one (false: centred on `fallback`).
///
/// The saved window's display is the one under its centre, else the one
/// showing its title bar (at least `MIN_VISIBLE` of the strip). The bounds are
/// kept when they fit that display, their centre is on it (GPUI requires
/// that) and the title bar shows; otherwise they are clamped to its size and
/// shifted onto it. On neither, the window is centred on `fallback`.
pub fn placement(saved: Bounds<Pixels>, displays: &[Bounds<Pixels>], fallback: usize) -> (Bounds<Pixels>, usize, bool) {
    const TITLE_BAR: f32 = 32.;
    const MIN_VISIBLE: f32 = 100.;
    let strip = Bounds::new(saved.origin, size(saved.size.width, px(TITLE_BAR)));
    let shows_title = |d: &Bounds<Pixels>| {
        let seen = strip.intersect(d).size;
        f32::from(seen.width) >= MIN_VISIBLE.min(f32::from(saved.size.width)) && f32::from(seen.height) >= TITLE_BAR / 2.
    };
    let home = displays.iter().position(|d| d.contains(&saved.center())).or_else(|| displays.iter().position(shows_title));
    let Some(home) = home else {
        let area = displays[fallback];
        return (Bounds::centered_at(area.center(), saved.size.min(&area.size)), fallback, false);
    };
    let area = displays[home];
    let fit = saved.size.min(&area.size);
    if fit == saved.size && area.contains(&saved.center()) && displays.iter().any(shows_title) {
        return (saved, home, true);
    }
    let x = saved.origin.x.clamp(area.left(), area.right() - fit.width);
    let y = saved.origin.y.clamp(area.top(), area.bottom() - fit.height);
    (Bounds::new(point(x, y), fit), home, true)
}

pub fn title(project: &str) -> String {
    format!("{project} — Aurelius")
}

/// Remembers where the project window is, once a move or resize settles.
pub fn save_bounds(view: &mut AppView, project: ProjectId, window: &mut Window, cx: &mut Context<AppView>) {
    let (bounds, maximized) = match window.window_bounds() {
        WindowBounds::Windowed(b) => (b, false),
        WindowBounds::Maximized(b) | WindowBounds::Fullscreen(b) => (b, true),
    };
    view.window_save = Some(cx.spawn(async move |_, cx| {
        cx.background_executor().timer(Duration::from_millis(400)).await;
        let mut all = crate::app::ui_prefs()["project_windows"].clone();
        if !all.is_object() {
            all = serde_json::json!({});
        }
        all[project.to_string()] = serde_json::json!({ "bounds": bounds, "maximized": maximized });
        crate::app::set_ui_value("project_windows", all);
    }));
}

#[cfg(test)]
mod tests {
    use super::placement;
    use gpui_kit::{point, px, size, Bounds, Pixels};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn screen() -> Bounds<Pixels> {
        rect(0., 0., 1920., 1040.)
    }

    fn second() -> Bounds<Pixels> {
        rect(1920., 0., 2560., 1400.)
    }

    fn narrow() -> Bounds<Pixels> {
        rect(0., 0., 992., 1850.)
    }

    #[test]
    fn fully_visible_stays() {
        let saved = rect(100., 100., 1180., 780.);
        assert_eq!(placement(saved, &[screen()], 0), (saved, 0, true));
    }

    #[test]
    fn partly_off_the_edge_stays_while_its_centre_is_on_the_display() {
        let saved = rect(1000., 300., 1180., 780.);
        assert_eq!(placement(saved, &[screen()], 0), (saved, 0, true));
    }

    #[test]
    fn centre_off_the_display_is_shifted_back_onto_it() {
        // GPUI would reset these to its default size: centre right of the edge.
        assert_eq!(placement(rect(700., 300., 900., 700.), &[narrow()], 0), (rect(92., 300., 900., 700.), 0, true));
        // Title bar above the top edge, centre still on screen.
        assert_eq!(placement(rect(100., -100., 1180., 780.), &[screen()], 0), (rect(100., 0., 1180., 780.), 0, true));
    }

    #[test]
    fn nothing_on_screen_centres_on_the_fallback() {
        let above = rect(100., -400., 1180., 780.);
        assert_eq!(placement(above, &[screen()], 0), (rect(370., 130., 1180., 780.), 0, false));
        let far = rect(-5000., 200., 1180., 780.);
        assert_eq!(placement(far, &[screen(), second()], 1), (rect(2610., 310., 1180., 780.), 1, false));
    }

    #[test]
    fn larger_than_the_display_is_clamped_to_it() {
        assert_eq!(placement(rect(100., 100., 3000., 4000.), &[narrow()], 0), (narrow(), 0, true));
        assert_eq!(placement(rect(3000., 0., 2500., 1400.), &[screen()], 0), (screen(), 0, false));
    }

    #[test]
    fn two_displays() {
        let both = [screen(), second()];
        // On the second display: kept there.
        let saved = rect(2200., 200., 1180., 780.);
        assert_eq!(placement(saved, &both, 0), (saved, 1, true));
        // That display gone: centred on the fallback.
        assert_eq!(placement(saved, &[screen()], 0), (rect(370., 130., 1180., 780.), 0, false));
        // Across both: the display under the centre holds it.
        let across = rect(1500., 100., 1180., 780.);
        assert_eq!(placement(across, &both, 0), (across, 1, true));
        // Centre below the shorter display, on neither: shifted onto the one with the title bar.
        assert_eq!(placement(rect(1000., 900., 1180., 780.), &both, 1), (rect(740., 260., 1180., 780.), 0, true));
    }
}
