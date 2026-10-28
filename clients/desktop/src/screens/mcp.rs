//! Settings → Advanced → MCP & tools: the browser (Claude in Chrome or Playwright, Chrome or Edge), Claude artifacts
//! on/off, the claude.ai account (artifact.rs), the MCP servers (on/off, Add and Remove, see mcp_add.rs) and the
//! Always allowed tools (always_allow.rs). The keys of apps that use Aurelius through aurelius-mcp have their own
//! page (app_keys.rs). All are server-wide and loaded when Settings opens or this page is shown.

use crate::app::AppView;
use crate::backend;
use crate::keymap::{LOAD, SettingsArtifacts, ToggleAutoAllow};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Settings → MCP & tools (after Plugins).
pub const MCP_TAB: usize = 8;

impl AppView {
    /// Fetch the browser choice, the MCP servers and the always allowed tools again.
    pub fn enter_mcp_tab(&mut self) {
        let _ = self.requests.send(backend::Request::LoadBrowser);
        let _ = self.requests.send(backend::Request::LoadArtifactSetting);
        if self.auto_allow_supported() {
            let _ = self.requests.send(backend::Request::LoadAutoAllow);
        }
        let _ = self.requests.send(backend::Request::LoadMcp);
        let _ = self.requests.send(backend::Request::LoadMcpAlwaysAllow);
    }

    pub fn render_mcp_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        // LOAD context: Tab and Esc only, no single-letter shortcuts on this page.
        let page = page.key_context(LOAD);
        let section = |title: &'static str| div().mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).child(ui::eyebrow(title));
        page.child(
            h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                .child(self.settings_title(cx)),
        )
        .child(
            v().id("mcp-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.))
                .child(ui::eyebrow("BROWSER"))
                // Older servers send no backend: they only have Claude in Chrome.
                .when_some(self.browser.as_ref().and_then(|s| s.as_ref().ok()).and_then(|s| s.backend), |el, current| {
                    use aurelius_protocol::BrowserBackend;
                    let label = |b: BrowserBackend| match b { BrowserBackend::ClaudeInChrome => "Claude in Chrome", BrowserBackend::Playwright => "Playwright" };
                    let key = |b: BrowserBackend| match b { BrowserBackend::ClaudeInChrome => "backend-chrome", BrowserBackend::Playwright => "backend-playwright" };
                    let note = match current {
                        BrowserBackend::ClaudeInChrome => "The extension in your own browser profile, with your logins. Asks before each browser action unless the mode is Auto.",
                        BrowserBackend::Playwright => "A separate Aurelius browser window on the server's computer: log in there once and the logins stay. Employees work in parallel, each in its own tab. Needs Node.js.",
                    };
                    el.child(
                        h().gap(px(12.)).items_center().text_size(px(13.))
                            .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child("Drive with"))
                            .child(
                                h().flex_none().p(px(2.)).gap(px(2.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER))
                                    .children([BrowserBackend::ClaudeInChrome, BrowserBackend::Playwright].into_iter().map(|b| {
                                        let on = current == b;
                                        div().id(key(b)).role(Role::Button).aria_label(SharedString::from(format!("Drive the browser with {}", label(b))))
                                            .px(px(10.)).py(px(3.)).rounded(px(5.)).cursor_pointer()
                                            .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                            .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                            .when(!on, |el| el.hover(|s| s.text_color(c(theme::TEXT))))
                                            .child(label(b))
                                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_browser_backend(b, cx)))
                                    })),
                            )
                            .child(ui::info("backend-about", note)),
                    )
                })
                .child({
                    use aurelius_protocol::BrowserChoice;
                    let label = |b: BrowserChoice| match b { BrowserChoice::Automatic => "Automatic", BrowserChoice::Chrome => "Chrome", BrowserChoice::Edge => "Edge" };
                    let key = |b: BrowserChoice| match b { BrowserChoice::Automatic => "browser-auto", BrowserChoice::Chrome => "browser-chrome", BrowserChoice::Edge => "browser-edge" };
                    let setting = self.browser.clone();
                    let current = setting.as_ref().and_then(|s| s.as_ref().ok()).map(|s| s.choice);
                    // Status and errors stay visible; what the choice means sits behind the info icon.
                    let (note, about): (Option<String>, Option<&str>) = match &setting {
                        None => (Some("Loading…".to_string()), None),
                        Some(Err(why)) => (Some(why.clone()), None),
                        Some(Ok(s)) => match s.env_override {
                            Some(forced) => (Some(format!("AURELIUS_BROWSER on the server forces {}", label(forced))), None),
                            None if s.choice == BrowserChoice::Automatic => (None, Some("Whichever is running, else the one that has the extension")),
                            None => (None, Some("Used from the next run")),
                        },
                    };
                    h().gap(px(12.)).items_center().text_size(px(13.))
                        .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child("Browser"))
                        .child(
                            h().flex_none().p(px(2.)).gap(px(2.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER))
                                .children([BrowserChoice::Automatic, BrowserChoice::Chrome, BrowserChoice::Edge].into_iter().map(|b| {
                                    let on = current == Some(b);
                                    div().id(key(b)).role(Role::Button).aria_label(SharedString::from(format!("Browser: {}", label(b))))
                                        .px(px(10.)).py(px(3.)).rounded(px(5.)).cursor_pointer()
                                        .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                        .when(!on, |el| el.hover(|s| s.text_color(c(theme::TEXT))))
                                        .child(label(b))
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_browser(b, cx)))
                                })),
                        )
                        .children(about.map(|text| ui::info("browser-about", text)))
                        .children(note.map(|text| div().flex_1().min_w_0().text_color(c(theme::MUTED)).child(text)))
                })
                .when_some(self.artifacts_enabled, |el, on| {
                    el.child(section("CLAUDE ARTIFACTS")).child(
                        h().gap(px(12.)).items_center().text_size(px(13.))
                            .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info(
                                "Claude artifacts",
                                "Employees can publish pages to claude.ai (private to your claude.ai account until you share them). Needs Claude Code signed in with claude.ai. Applies from the next run.",
                            )))
                            .child(ui::toggle("artifacts-enabled", on, "Claude artifacts", || Box::new(SettingsArtifacts))),
                    )
                })
                .when(self.auto_allow_supported(), |el| {
                    let allowed = self.may_approve();
                    el.child(section("AUTO-ALLOW SAFE TOOLS")).when_some(self.auto_allow.clone(), |el, setting| {
                        el.child(
                            h().gap(px(12.)).items_center().text_size(px(13.))
                                .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child("Auto-allow safe tools"))
                                .child(ui::toggle("auto-allow-enabled", setting.enabled, "Auto-allow safe tools", || Box::new(ToggleAutoAllow)).when(!allowed, |el| el.opacity(0.45)))
                                .child(div().flex_1().min_w_0().text_color(c(theme::MUTED)).child(if allowed {
                                    "These read-only tools run without asking, in every mode except Plan. WebFetch and screenshots still ask. Applies from the next run."
                                } else {
                                    "These read-only tools run without asking, in every mode except Plan. WebFetch and screenshots still ask. Changing it needs the Approve right on this server."
                                })),
                        )
                        .child(
                            h().gap(px(6.)).flex_wrap().pl(px(142.))
                                .children(setting.tools.iter().map(|t| {
                                    div().px(px(8.)).py(px(2.)).rounded(px(5.)).border_1().border_color(c(theme::BORDER)).text_size(px(12.))
                                        .text_color(c(if setting.enabled { theme::TEXT } else { theme::MUTED })).child(t.clone())
                                })),
                        )
                    })
                })
                .child(section("CLAUDE.AI ACCOUNT"))
                .child(
                    h().gap(px(12.)).items_center().text_size(px(13.))
                        .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child("claude.ai account"))
                        .child(ui::button("claude-ai-sign-in", "Sign in", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.open_claude_ai(crate::screens::artifact::CLAUDE_LOGIN.into(), "claude.ai account".into(), window, cx)
                        })))
                        .child(ui::button("claude-ai-sign-out", "Sign out", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.claude_sign_out(window, cx))))
                        .child(ui::info("claude-ai-about", "Docs, Slides, Design and Dashboards from claude.ai open inside Aurelius once you sign in here (Google works). The sign-in stays on this PC, apart from your browser.")),
                )
                .child(div().mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).child(h().gap(px(6.)).items_center().child(ui::eyebrow("MCP SERVERS")).child(ui::info(
                    "mcp-servers-about",
                    "On = every chat, conversation, employee and task can use the server. Applies from the next run.",
                ))))
                .map(|el| {
                    let blocked = self.mcp_manage_blocked();
                    el.child(
                        h().gap(px(12.)).items_center()
                            .child(ui::button("mcp-add", "Add MCP server", ButtonKind::Outline, None).when(blocked.is_some(), |el| el.opacity(0.45))
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_mcp(window, cx))))
                            .children(blocked.map(|why| div().flex_1().min_w_0().text_size(px(11.5)).text_color(c(theme::MUTED)).child(why))),
                    )
                })
                .children(match &self.mcp {
                    None => vec![div().text_size(px(13.)).text_color(c(theme::MUTED)).child("Loading…")],
                    Some(Err(why)) => vec![div().text_size(px(13.)).text_color(c(theme::MUTED)).child(why.clone())],
                    Some(Ok(list)) if list.servers.is_empty() && list.missing.is_empty() => vec![div().text_size(px(13.)).text_color(c(theme::MUTED))
                        .child("No MCP servers found. Add one with `claude mcp add --scope user`.")],
                    Some(Ok(list)) => {
                        let supported = crate::screens::mcp_add::login_supported(list);
                        let mut rows: Vec<Div> = list.servers.iter().enumerate().map(|(i, s)| {
                            let name = s.name.clone();
                            let on = s.enabled;
                            v().gap(px(6.)).child(h().gap(px(12.)).items_center().text_size(px(13.))
                                .child(
                                    div().id(SharedString::from(format!("mcp-{i}"))).role(Role::Button)
                                        .aria_label(SharedString::from(format!("MCP server {}: {}", s.name, if on { "on" } else { "off" })))
                                        .w(px(130.)).flex_none().px(px(10.)).py(px(3.)).rounded(px(5.)).cursor_pointer().border_1().border_color(c(theme::BORDER))
                                        .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                        .child(format!("{} · {}", s.name, if on { "On" } else { "Off" }))
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_mcp(&name, cx))),
                                )
                                .child(div().flex_1().min_w_0().text_color(c(theme::MUTED)).child(format!("{:?} · {}", s.transport, s.summary).to_lowercase()))
                                .children(self.mcp_auth_controls(s, i, supported, cx))
                                .map(|el| {
                                    let asking = self.mcp_manage.confirm_remove.as_deref() == Some(s.name.as_str());
                                    let removing = self.mcp_manage.removing.as_deref() == Some(s.name.as_str());
                                    let allowed = self.mcp_manage_blocked().is_none();
                                    let (ask, yes, keep) = (s.name.clone(), s.name.clone(), s.name.clone());
                                    if removing {
                                        return el.child(div().text_color(c(theme::MUTED)).child("Removing…"));
                                    }
                                    if asking {
                                        return el
                                            .child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(format!("Remove {} from Claude Code?", yes)))
                                            .child(ui::button(("mcp-remove-yes", i), "Remove", ButtonKind::Primary, None).h(px(28.))
                                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_mcp(&yes, cx))))
                                            .child(ui::button(("mcp-remove-no", i), "Keep", ButtonKind::Outline, None).h(px(28.))
                                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                                    if this.mcp_manage.confirm_remove.as_deref() == Some(keep.as_str()) {
                                                        this.mcp_manage.confirm_remove = None;
                                                    }
                                                    cx.notify();
                                                })));
                                    }
                                    el.child(ui::button(("mcp-remove-ask", i), "Remove", ButtonKind::Outline, None).h(px(28.)).text_color(c(theme::NEGATIVE)).when(!allowed, |el| el.opacity(0.45))
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                            if this.mcp_manage_blocked().is_none() {
                                                this.mcp_manage.confirm_remove = Some(ask.clone());
                                                cx.notify();
                                            }
                                        })))
                                }))
                                .children(self.mcp_login_panel(s, cx))
                        }).collect();
                        rows.extend(list.missing.iter().enumerate().map(|(i, name)| {
                            let n = name.clone();
                            h().gap(px(12.)).items_center().text_size(px(13.))
                                .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(name.clone()))
                                .child(div().flex_1().min_w_0().text_color(c(theme::MUTED)).child("Claude Code no longer has this server."))
                                .child(ui::button(SharedString::from(format!("mcp-remove-{i}")), "Remove", ButtonKind::Outline, None)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_missing_mcp(&n, cx))))
                        }));
                        rows
                    }
                })
                .children(self.mcp_error.clone().map(|why| div().text_size(px(12.)).text_color(c(theme::MUTED)).child(why)))
                .children(self.render_always_allowed(cx)),
        )
    }
}
