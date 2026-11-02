//! Aurelius desktop (GPUI). Owns windows and input; the daemon owns state.
// Release builds are GUI programs: no console window (dev keeps its console for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod keymap;
mod open_link;
mod preview;
mod project_window;
mod resize;
mod theme;
mod ui;
mod screens {
    pub mod chat;
    pub mod group_chat;
    pub mod projects;
    pub mod my_tickets;
    pub mod message_edit;
    pub mod sidebar;
    pub mod dashboard;
    pub mod employees;
    pub mod tasks;
    pub mod activity;
    pub mod manager;
    pub mod remote;
    pub mod load;
    pub mod plugins;
    pub mod schedules;
    pub mod project_settings;
    pub mod mcp;
    pub mod mcp_add;
    pub mod always_allow;
    pub mod app_keys;
    pub mod servers;
    pub mod server_name;
    pub mod overlays;
    pub mod approval_plan;
    pub mod approval;
    pub mod side_panels;
    pub mod trace;
    pub mod progress;
    pub mod markdown;
    pub mod mermaid;
    pub mod artifact;
    pub mod org;
    pub mod board_members;
    pub mod account;
    pub mod board;
    pub mod ticket;
    pub mod discussion;
    pub mod team_feed;
    pub mod threads;
    pub mod who;
    pub mod links;
    pub mod updates;
    pub mod thread_panel;
    pub mod ticket_discussion;
    pub mod vault;
    pub mod guidelines;
}

use gpui_kit::*;

/// The app is released apart from the server: the local service, which
/// installs app updates, reads the installed app's version from this file
/// (`aurelius_host::updater::DESKTOP_VERSION_FILE`) beside `aurelius.exe`.
fn record_version() {
    let Ok(dir) = aurelius_host::startup::installed_dir() else { return };
    let path = dir.join("aurelius.version");
    let version = env!("CARGO_PKG_VERSION");
    if std::fs::read_to_string(&path).map_or(true, |v| v.trim() != version) {
        let _ = std::fs::write(path, version);
    }
}

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    record_version();

    // A link the OS handed over (A5b): with a running app it goes there and this process ends.
    let (links, mut incoming) = open_link::channel();
    let data_root = aurelius_host::data_dir().ok();
    let first = open_link::link_arg(std::env::args());
    let running = data_root.as_deref().is_some_and(|root| open_link::pass_to_running(root, first.as_deref().unwrap_or("")));
    if running && first.is_some() {
        return;
    }
    if let Some(url) = first {
        let _ = links.send(url);
    }
    if !running && let Some(root) = data_root.as_deref() {
        open_link::listen(root, links.clone());
    }

    let application = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    // macOS hands a clicked link over as an event, not as an argument.
    application.on_open_urls({
        let links = links.clone();
        move |urls| urls.into_iter().for_each(|url| drop(links.send(url)))
    });
    application.run(move |cx| {
        gpui_kit::init(cx);
        theme::apply(cx);
        keymap::bind(cx);
        // The hovered link's URL goes to the status line of the window it is in.
        gpui_kit::base::text::on_link_hover(cx, |url, window, cx| {
            let Some(Some(root)) = window.root::<gpui_kit::base::Root>() else { return };
            if let Ok(view) = root.read(cx).view().clone().downcast::<app::AppView>() {
                view.update(cx, |view, cx| view.set_hovered_link(url, cx));
            }
        });

        // Closing the main window ends the UI process; the daemon keeps running.
        project_window::track_closes(cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
            window_min_size: Some(size(px(900.), px(640.))),
            // Own title bar in the app's style (drag area + window controls).
            titlebar: Some(TitlebarOptions {
                title: Some(if aurelius_host::Environment::current() == aurelius_host::Environment::Dev { "Aurelius Dev".into() } else { "Aurelius".into() }),
                appears_transparent: true,
                // macOS: the traffic lights, vertically centred in our 48 px bar.
                traffic_light_position: Some(point(px(14.), px(17.))),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (main, _) = gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| app::AppView::new(window, cx)))
            .expect("failed to open the Aurelius window");
        cx.default_global::<project_window::Windows>().main = Some(main.window_id());

        // Links wait here until the window exists; the app holds them until the server is connected.
        cx.spawn(async move |cx| {
            while let Some(url) = incoming.recv().await {
                let _ = main.update(cx, |_, window, cx| {
                    let Some(Some(root)) = window.root::<gpui_kit::base::Root>() else { return };
                    if let Ok(view) = root.read(cx).view().clone().downcast::<app::AppView>() {
                        view.update(cx, |view, cx| view.os_link_received(url, cx));
                    }
                });
            }
        })
        .detach();
    });
}
