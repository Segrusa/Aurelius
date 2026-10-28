//! Settings → Remote access: turn the gateway on, pair
//! a phone (QR code) or another computer (pairing text, full access), allow or
//! refuse it, see and revoke devices, and set up push notifications (the PC
//! sends them to Apple itself).

use crate::app::AppView;
use crate::backend::{self, RemoteCall};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;
use std::time::Duration;

pub const REMOTE_TAB: usize = 3;

/// One step of push setup: the APNs key file is chosen; the Team ID is asked next.
#[derive(Clone)]
pub struct PendingPushKey {
    pub key_id: String,
    pub key_p8: String,
}

/// The QR code as dark/light modules, painted as squares.
fn qr(payload: &str, module: f32) -> impl IntoElement {
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::M).ok();
    let (width, dark): (usize, Vec<bool>) = match &code {
        Some(code) => (code.width(), code.to_colors().into_iter().map(|m| m == qrcode::Color::Dark).collect()),
        None => (0, Vec::new()),
    };
    let quiet = 2usize;
    let side = (width + quiet * 2) as f32 * module;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(fill(bounds, c(0xFFFFFF)));
            let o = bounds.origin;
            let m = px(module);
            for (i, is_dark) in dark.iter().enumerate() {
                if *is_dark {
                    let (x, y) = ((i % width + quiet) as f32, (i / width + quiet) as f32);
                    window.paint_quad(fill(Bounds::new(point(o.x + m * x, o.y + m * y), size(m, m)), c(0x111210)));
                }
            }
        },
    )
    .size(px(side))
    .flex_none()
}

impl AppView {
    pub fn on_open_remote(&mut self, _: &OpenRemoteAccess, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(&OpenSettings, window, cx);
        self.enter_remote_tab(cx);
    }

    /// Show the tab and keep its status fresh while it is open (pairing
    /// requests arrive from the phone; the desktop polls, it is cheap).
    pub fn enter_remote_tab(&mut self, cx: &mut Context<Self>) {
        self.settings_tab = REMOTE_TAB;
        self.settings_selected = 0;
        let _ = self.requests_handle().send(backend::Request::Remote(RemoteCall::Status));
        self.remote_poll += 1;
        let epoch = self.remote_poll;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let keep = this
                    .update(cx, |this, _| {
                        let open = this.screen == crate::app::Page::Settings && this.settings_tab == REMOTE_TAB && this.remote_poll == epoch;
                        if open {
                            let _ = this.requests_handle().send(backend::Request::Remote(RemoteCall::Status));
                        }
                        open
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub fn remote_toggle(&mut self, cx: &mut Context<Self>) {
        if self.settings_tab != REMOTE_TAB {
            return;
        }
        let on = self.remote.as_ref().is_some_and(|r| r.enabled);
        let call = if on { RemoteCall::Disable } else { RemoteCall::Enable };
        let _ = self.requests_handle().send(backend::Request::Remote(call));
        self.pairing = None;
        self.notify_user(
            if on { "Turning remote access off. Phones disconnect." } else { "Turning remote access on (Tailscale Serve, this tailnet only)…" },
            false,
            cx,
        );
    }

    pub fn remote_pair(&mut self, cx: &mut Context<Self>) {
        if !self.remote.as_ref().is_some_and(|r| r.enabled) {
            self.notify_user("Turn remote access on first (O).", true, cx);
            return;
        }
        let _ = self.requests_handle().send(backend::Request::Remote(RemoteCall::PairingStart));
    }

    /// Allow (or refuse) the oldest waiting device. A computer asking for
    /// full access gets it with Allow: the card says so before Enter.
    pub fn remote_decide(&mut self, approve: bool, cx: &mut Context<Self>) {
        let Some(request) = self.remote.as_ref().and_then(|r| r.pending.first()).cloned() else {
            self.notify_user("No device is waiting. Scan the code with the Aurelius app, or paste the pairing text on the other computer.", false, cx);
            return;
        };
        let owner = approve && request.owner;
        let _ = self.requests_handle().send(backend::Request::Remote(RemoteCall::Decide { pairing_id: request.pairing_id, approve, owner }));
        self.pairing = None;
        let text = match (approve, owner) {
            (true, true) => format!("{} is paired with full access.", request.device_name),
            (true, false) => format!("{} is paired.", request.device_name),
            _ => format!("{} was refused.", request.device_name),
        };
        self.notify_user(text, false, cx);
    }

    /// Copy the pairing text for Aurelius on another computer (Settings → Servers there).
    pub fn remote_copy_pairing(&mut self, cx: &mut Context<Self>) {
        if self.settings_tab != REMOTE_TAB {
            return;
        }
        match self.pairing.as_ref().map(|p| p.pairing_text.clone()).filter(|t| !t.is_empty()) {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.notify_user("Pairing text copied. On the other computer: Settings → Servers (Ctrl Alt S) → N, paste, Enter.", false, cx);
            }
            None => self.notify_user("Press N for a pairing code first.", false, cx),
        }
    }

    pub fn remote_revoke(&mut self, cx: &mut Context<Self>) {
        let Some(device) = self.remote.as_ref().and_then(|r| r.clients.get(self.settings_selected)).cloned() else { return };
        let me = match &self.connection {
            backend::Connection::Online { server, .. } => server.client_id,
            _ => None,
        };
        if me == Some(device.id) {
            self.notify_user("That is this computer. To stop using this server, forget it in Settings → Servers (Ctrl Alt S).", false, cx);
            return;
        }
        let _ = self.requests_handle().send(backend::Request::Remote(RemoteCall::Revoke(device.id)));
        self.notify_user(format!("{} removed. Its connection closes now.", device.name), false, cx);
    }

    pub fn remote_push_test(&mut self, cx: &mut Context<Self>) {
        if self.settings_tab != REMOTE_TAB {
            return;
        }
        match self.remote.as_ref().and_then(|r| r.clients.get(self.settings_selected)).cloned() {
            Some(device) if device.push => {
                let _ = self.requests_handle().send(backend::Request::Remote(RemoteCall::PushTest(device.id)));
            }
            Some(device) => self.notify_user(format!("{} has not turned on notifications (in the app: More → Notifications).", device.name), false, cx),
            None => self.notify_user("No paired phone selected.", false, cx),
        }
    }

    /// Choose the APNs key (AuthKey_<KEYID>.p8); the Team ID is asked next.
    pub fn remote_push_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_tab != REMOTE_TAB {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("APNs key (.p8)".into()) });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let key_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.trim_start_matches("AuthKey_").to_string())
                .unwrap_or_default();
            let content = std::fs::read_to_string(&path);
            let _ = this.update_in(cx, |this, window, cx| match content {
                Ok(key_p8) if key_p8.contains("PRIVATE KEY") => {
                    this.pending_push = Some(PendingPushKey { key_id, key_p8 });
                    this.entering_key = true;
                    this.team_input.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.focus(window, cx);
                    });
                    this.notify_user("Now type your Apple Team ID (10 characters, developer.apple.com → Membership) and press Enter.", false, cx);
                    cx.notify();
                }
                _ => this.notify_user("That is not an APNs key file (.p8).", true, cx),
            });
        })
        .detach();
    }

    /// Enter in the Team ID field: store the key and ids on the PC.
    pub fn save_push_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        let Some(key) = self.pending_push.take() else { return };
        let bundle_id = self.remote.as_ref().and_then(|r| r.push.bundle_id.clone()).unwrap_or_else(|| "com.segrusa.aurelius".into());
        let config = aurelius_protocol::remote::PushConfig { key_id: key.key_id, team_id, bundle_id, key_p8: Some(key.key_p8) };
        let _ = self.requests_handle().send(backend::Request::Remote(RemoteCall::PushConfigure(config)));
        self.notify_user("APNs key stored in the Windows credential store.", false, cx);
    }

    pub fn render_remote_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        let status = self.remote.clone().unwrap_or_default();
        let mono = ui::mono(cx);
        let row = |label: &'static str, value: String| {
            h().gap(px(12.)).items_start().text_size(px(13.)).child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(label)).child(div().flex_1().child(value))
        };
        let tailscale = &status.tailscale;
        let tailscale_line = match (&tailscale.error, tailscale.online, tailscale.serving) {
            (Some(e), _, _) => e.clone(),
            (None, true, true) => format!("Connected · serving {}", status.url.clone().unwrap_or_default()),
            (None, true, false) => "Connected · not serving Aurelius yet".into(),
            (None, false, _) => "Not connected".into(),
        };
        let selected = self.settings_selected.min(status.clients.len().saturating_sub(1));

        let pairing = self.pairing.clone().filter(|_| status.enabled);
        let pending = status.pending.first().cloned();

        page.child(
            h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                .child(self.settings_title(cx)),
        )
        .child(
            v().id("remote-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.))
                .child(row("Remote access", if status.enabled { "On (O turns it off)".into() } else { "Off (O turns it on)".into() }))
                .child(row("Tailscale", tailscale_line))
                .when_some(status.error.clone(), |el, e| el.child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(e)))
                .when_some(pending.clone(), |el, request| {
                    let (text, allow) = if request.owner {
                        (
                            format!(
                                "{} ({}) asks for full access: it can do everything this app does here, including API keys, folders on this PC, backups, updates and remote access.",
                                request.device_name, request.platform
                            ),
                            "Allow as owner",
                        )
                    } else {
                        (format!("{} ({}) wants to connect to this PC.", request.device_name, request.platform), "Allow")
                    };
                    el.child(
                        h().gap(px(12.)).px(px(14.)).py(px(12.)).rounded(px(8.)).bg(c(theme::ATTENTION_BG)).border_1().border_color(c(if request.owner { theme::NEGATIVE } else { theme::ATTENTION_BORDER }))
                            .child(ui::pulse_dot("pair-request", 7., theme::ACCENT))
                            .child(div().flex_1().text_color(c(theme::ACCENT_TEXT)).child(text))
                            .child(ui::button("pair-refuse", "Refuse", ButtonKind::Outline, Some(ui::kbd("x", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.remote_decide(false, cx))))
                            .child(ui::button("pair-allow", allow, ButtonKind::Primary, Some(ui::kbd("enter", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.remote_decide(true, cx)))),
                    )
                })
                .when_some(pairing.filter(|_| pending.is_none()), |el, code| {
                    let minutes = ((code.expires_at_ms - aurelius_storage_now()) / 60_000).max(0) + 1;
                    el.child(
                        h().gap(px(20.)).items_start().pt(px(4.))
                            .child(qr(&code.qr, 4.))
                            .child(
                                v().gap(px(8.)).flex_1()
                                    .child(h().gap(px(8.)).items_center().child(div().font_family(ui::serif(cx)).text_size(px(18.)).child("Scan with the Aurelius app"))
                                        .child(ui::info("scan-about", "Open Aurelius on the phone and choose Scan QR code. You confirm the phone here before it gets access.")))
                                    .child(h().gap(px(8.)).child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Or type the code:")).child(
                                        div().font_family(mono.clone()).text_size(px(16.)).child(format!("{}-{}", &code.code[..4.min(code.code.len())], &code.code[4.min(code.code.len())..])),
                                    ))
                                    .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("Valid for {minutes} min, once.")))
                                    .when(!code.pairing_text.is_empty(), |el| {
                                        el.child(h().gap(px(8.)).pt(px(6.))
                                            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Another computer:"))
                                            .child(ui::button("pair-copy", "Copy pairing text", ButtonKind::Outline, Some(ui::kbd("y", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.remote_copy_pairing(cx)))))
                                    }),
                            ),
                    )
                })
                .child(ui::eyebrow("PAIRED DEVICES").pt(px(6.)))
                .when(status.clients.is_empty(), |el| el.child(div().text_color(c(theme::MUTED)).text_size(px(12.5)).child("No devices yet. Press N to pair a phone or computer.")))
                .children(status.clients.iter().enumerate().map(|(i, d)| {
                    let seen = if d.connected { "connected now".to_string() } else { d.last_seen_ms.map(ui::fmt_when).map(|w| format!("last seen {w}")).unwrap_or_else(|| "never connected".into()) };
                    let owner = d.capabilities.contains(&aurelius_protocol::remote::Capability::Owner);
                    let me = matches!(&self.connection, backend::Connection::Online { server, .. } if server.client_id == Some(d.id));
                    let badge = match (owner, me) {
                        (true, true) => "full access · this computer",
                        (true, false) => "full access",
                        (false, _) => "",
                    };
                    h().id(("remote-device", i)).gap(px(12.)).px(px(12.)).py(px(9.)).rounded(px(6.)).cursor_pointer()
                        .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.settings_selected = i;
                            cx.notify();
                        }))
                        .child(if d.connected { ui::dot(7., theme::ACCENT) } else { ui::ring(7., theme::MUTED) })
                        .child(div().w(px(180.)).flex_none().truncate().text_size(px(13.)).child(d.name.clone()))
                        .child(div().w(px(70.)).flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(d.platform.clone()))
                        .child(div().flex_1().text_size(px(11.5)).text_color(c(theme::MUTED)).child(seen))
                        .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(if d.push { "notifications on" } else { "" }))
                        .child(div().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child(badge))
                }))
                .child(h().gap(px(6.)).items_center().pt(px(6.)).child(ui::eyebrow("NOTIFICATIONS")).child(ui::info("apns-about", "Push notifications need your Apple key: choose your AuthKey_….p8 from developer.apple.com (Keys → Apple Push Notifications).")))
                .child(row(
                    "Apple (APNs)",
                    if status.push.apns {
                        format!("Set up · key {} · team {}", status.push.key_id.clone().unwrap_or_default(), status.push.team_id.clone().unwrap_or_default())
                    } else {
                        "Not set up. Press A to add your key.".into()
                    },
                ))
                .when_some(status.push.last_error.clone(), |el, e| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
                .when(self.entering_key && self.pending_push.is_some(), |el| {
                    el.child(v().gap(px(6.)).key_context("SettingsKey").child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Apple Team ID")).child(Input::new(&self.team_input)))
                }),
        )
        .child(
            h().px(px(22.)).py(px(12.)).gap(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                .child(ui::hint("r-toggle", "o", if status.enabled { "Turn off" } else { "Turn on" }, || Box::new(SettingsRemoteToggle), cx))
                .child(ui::hint("r-pair", "n", "Pair a device", || Box::new(SettingsNew), cx))
                .child(ui::hint("r-copy", "y", "Copy pairing text", || Box::new(SettingsCopyPairing), cx))
                .child(ui::key_hint(&["down"], "select", cx))
                .child(ui::hint("r-revoke", "delete", "Remove device", || Box::new(SettingsRemove), cx))
                .child(ui::hint("r-key", "a", "APNs key…", || Box::new(SettingsPushKey), cx))
                .child(ui::hint("r-test", "t", "Test notification", || Box::new(SettingsPushTest), cx)),
        )
    }
}

fn aurelius_storage_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
