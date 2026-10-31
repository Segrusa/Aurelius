//! Settings → Advanced → App keys: keys that let an outside app or agent use
//! Aurelius through `aurelius-mcp` (feature `app_keys`, owner right). Each key has
//! scopes and limits; its secret is shown once, and revoking works at once. There
//! are no events, so the list is asked again on entering the page and after each change.

use crate::app::AppView;
use crate::backend::{self, AppKeyCall, AppKeyUpdate, Connection};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::AppKeyId;
use aurelius_domain::app_key::{AppAudience, AppKeyCreated, AppKeyView, AppScope, CreateAppKey, validate_webhook_url};
use gpui_kit::component::input::InputState;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Settings → Advanced → App keys.
pub const APP_KEYS_TAB: usize = 6;

pub const TOO_OLD: &str = "This server is too old for app keys. Update the server to let other apps use Aurelius through aurelius-mcp.";
pub const NO_OWNER: &str = "App keys need the owner right on this server.";

/// "3 of 10" / "3 · no limit".
pub fn usage_text(used: u32, limit: Option<u32>) -> String {
    match limit {
        Some(limit) => format!("{used} of {limit}"),
        None => format!("{used} · no limit"),
    }
}

/// A limit field: empty is unlimited, else a whole number from 1.
pub fn parse_limit(text: &str, what: &str) -> Result<Option<u32>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(Some(n)),
        _ => Err(format!("{what} must be a whole number from 1, or empty for no limit.")),
    }
}

pub(crate) fn chip(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    h().id(id)
        .h(px(28.))
        .px(px(10.))
        .rounded(px(6.))
        .border_1()
        .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
        .when(on, |el| el.bg(c(theme::SURFACE_2)))
        .cursor_pointer()
        .text_size(px(12.5))
        .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
        .hover(|s| s.border_color(c(theme::BORDER_STRONG)).text_color(c(theme::TEXT)))
        .child(label.into())
}

pub(crate) fn labelled(label: &'static str, body: impl IntoElement) -> Div {
    v().gap(px(6.)).child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(label)).child(body)
}

/// `labelled` with the explanation behind an info icon.
pub(crate) fn labelled_info(label: &'static str, about: &'static str, body: impl IntoElement) -> Div {
    v().gap(px(6.)).child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::label_with_info(label, about))).child(body)
}

pub(crate) fn notice(text: impl Into<SharedString>, bad: bool) -> Div {
    h().px(px(12.)).py(px(10.)).rounded(px(8.)).border_1()
        .border_color(c(if bad { theme::NEGATIVE } else { theme::ATTENTION_BORDER }))
        .bg(c(theme::ATTENTION_BG))
        .text_size(px(12.5))
        .text_color(c(if bad { theme::NEGATIVE } else { theme::ACCENT_TEXT }))
        .child(div().flex_1().min_w_0().child(text.into()))
}

pub struct AppKeyEditor {
    pub name: Entity<InputState>,
    pub scopes: Vec<AppScope>,
    pub limit_hour: Entity<InputState>,
    pub limit_open: Entity<InputState>,
    pub audience: AppAudience,
    pub webhook: Entity<InputState>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct AppKeysState {
    pub keys: Vec<AppKeyView>,
    pub loaded: bool,
    pub editor: Option<AppKeyEditor>,
    /// A key that was just made: its secret is shown here once.
    pub created: Option<AppKeyCreated>,
    pub confirm_revoke: Option<AppKeyId>,
    pub error: Option<String>,
}

/// What a scope allows, for the New key form.
pub fn scope_help(scope: AppScope) -> &'static str {
    match scope {
        AppScope::Read => "read: projects, employees, tasks, the inbox, ideas and memory",
        AppScope::Draft => "draft: tasks as drafts (no employee), ideas and memories",
        AppScope::Run => "run: tasks that start at once, messages to tasks, stopping tasks, the project assistant (includes draft)",
        AppScope::Approve => "approve: Auto mode and answering approvals. Without it an app's tasks never run in Auto",
    }
}

/// What an audience means, for the New key form.
pub fn audience_help(audience: AppAudience) -> &'static str {
    match audience {
        AppAudience::All => "all: everything its scopes allow",
        AppAudience::Management => "management: only CEO/CTO/CMO level, no employees below them (no approve scope)",
    }
}

/// The webhook URL field as the command wants it: empty is no webhook, anything else must be http(s).
pub fn webhook_field(text: &str) -> Result<Option<String>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    validate_webhook_url(text).map(Some).map_err(|_| "The webhook URL must start with http:// or https://, have a host and contain no spaces.".to_string())
}

/// "read, run" with draft left out when run already includes it.
pub fn scopes_text(scopes: &[AppScope]) -> String {
    let shown = scopes.iter().filter(|s| !(**s == AppScope::Draft && scopes.contains(&AppScope::Run)));
    shown.map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}

/// The `claude mcp add` commands for a new key: on the server PC (stdio) and over the gateway.
pub fn connect_commands(gateway: &str, secret: &str) -> (String, String) {
    let gateway = gateway.trim_end_matches('/');
    (
        format!("claude mcp add aurelius --scope user -e AURELIUS_MCP_KEY={secret} -- aurelius-mcp"),
        format!("claude mcp add --transport http aurelius {gateway}/v1/mcp --header \"Authorization: Bearer {secret}\""),
    )
}

impl AppView {
    /// Why app keys cannot be managed now; `None` when they can.
    pub fn app_keys_unavailable(&self) -> Option<&'static str> {
        match &self.connection {
            Connection::Online { server, .. } if !server.app_keys => Some(TOO_OLD),
            Connection::Online { server, .. } if !server.owner => Some(NO_OWNER),
            Connection::Online { .. } => None,
            _ => Some("Not connected to a server."),
        }
    }

    pub fn app_keys_load(&mut self) {
        self.app_keys.created = None;
        self.app_keys.editor = None;
        self.app_keys.confirm_revoke = None;
        self.app_keys.error = None;
        self.app_keys_send(AppKeyCall::List);
    }

    fn app_keys_send(&mut self, call: AppKeyCall) {
        match self.app_keys_unavailable() {
            Some(reason) => self.app_keys.error = Some(reason.into()),
            None => { let _ = self.requests_handle().send(backend::Request::AppKeys(call)); }
        }
    }

    pub fn app_keys_update(&mut self, update: AppKeyUpdate, cx: &mut Context<Self>) {
        match update {
            AppKeyUpdate::Keys(keys) => {
                self.app_keys.keys = keys;
                self.app_keys.loaded = true;
                self.app_keys.confirm_revoke = None;
            }
            AppKeyUpdate::Created(created) => {
                self.app_keys.editor = None;
                self.app_keys.error = None;
                self.app_keys.created = Some(created);
            }
            AppKeyUpdate::Failed(why) => match self.app_keys.editor.as_mut() {
                Some(editor) => editor.error = Some(why),
                None => self.app_keys.error = Some(why),
            },
        }
        cx.notify();
    }

    fn app_key_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        self.app_keys.created = None;
        self.app_keys.editor = Some(AppKeyEditor {
            name: input("Name, for example 'Assistant'", window, cx),
            scopes: vec![AppScope::Read],
            limit_hour: input("No limit", window, cx),
            limit_open: input("No limit", window, cx),
            audience: AppAudience::All,
            webhook: input("https://… (empty: no webhook)", window, cx),
            error: None,
        });
        cx.notify();
    }

    fn app_key_save(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.app_keys.editor.as_ref() else { return };
        let text = |i: &Entity<InputState>| i.read(cx).value().to_string();
        let name = text(&e.name).trim().to_string();
        let cmd = (|| {
            if name.is_empty() {
                return Err("Give the key a name: tasks of this app show it.".to_string());
            }
            if e.scopes.is_empty() {
                return Err("Choose at least one scope.".to_string());
            }
            let (management, hooks) = match &self.connection { Connection::Online { server, .. } => (server.management_channel, server.webhooks), _ => (false, false) };
            let audience = if management { e.audience } else { AppAudience::All };
            if audience == AppAudience::Management && e.scopes.contains(&AppScope::Approve) {
                return Err("A management key cannot have the approve scope.".to_string());
            }
            let webhook_url = if hooks { webhook_field(&text(&e.webhook))? } else { None };
            Ok(CreateAppKey {
                name,
                scopes: e.scopes.clone(),
                limit_per_hour: parse_limit(&text(&e.limit_hour), "Tasks per hour")?,
                limit_concurrent: parse_limit(&text(&e.limit_open), "Tasks open at once")?,
                audience,
                webhook_url,
            })
        })();
        if let Some(e) = self.app_keys.editor.as_mut() {
            e.error = cmd.as_ref().err().cloned();
        }
        if let Ok(cmd) = cmd {
            self.app_keys_send(AppKeyCall::Create(cmd));
        }
        cx.notify();
    }

    /// Settings → Advanced → App keys: the page.
    pub fn render_app_keys_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        // LOAD context: Tab and Esc only, no single-letter shortcuts on this page.
        page.key_context(crate::keymap::LOAD)
            .child(h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx)))
            .child(v().id("app-keys-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.)).children(self.render_app_keys(cx)))
    }

    /// The App keys list: what they are for, the keys, and making a new one.
    pub fn render_app_keys(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = vec![
            div().mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).child(h().gap(px(6.)).items_center().child(ui::eyebrow("APPS (MCP)")).child(ui::info(
                "app-keys-about",
                "An app key lets another app or agent (Claude Code elsewhere, the assistant) use Aurelius through aurelius-mcp, with only the scopes you pick. Make one key per app, so you can revoke it alone. Setup: docs/aurelius-mcp.md in the Aurelius repository.",
            ))).into_any_element(),
        ];
        if let Some(reason) = self.app_keys_unavailable() {
            out.push(notice(reason, false).into_any_element());
            return out;
        }
        let mono = ui::mono(cx);
        let code = move |text: String| div().px(px(10.)).py(px(8.)).rounded(px(6.)).bg(c(theme::SURFACE_2)).font_family(mono.clone()).text_size(px(11.5)).line_height(px(17.)).child(text);
        let copy_row = |id: &'static str, text: String, what: &'static str, cx: &mut Context<Self>| {
            let copied = text.clone();
            h().gap(px(10.)).items_start().child(div().flex_1().min_w_0().child(code(text)))
                .child(ui::button(id, "Copy", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                    this.notify_user(format!("{what} copied."), false, cx);
                })))
        };

        out.push(
            h().gap(px(8.))
                .child(div().flex_1())
                .child(ui::button("app-keys-refresh", "Refresh", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.app_keys_send(AppKeyCall::List);
                    cx.notify();
                })))
                .child(ui::button("app-key-new", "New app key", ButtonKind::Primary, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.app_key_new(w, cx))))
                .into_any_element(),
        );
        if let Some(why) = self.app_keys.error.clone() {
            out.push(notice(why, true).into_any_element());
        }

        if let Some(created) = self.app_keys.created.clone() {
            let gateway = self.remote.as_ref().and_then(|r| r.url.clone()).unwrap_or_else(|| "https://<this-pc>.<tailnet>.ts.net:8443".into());
            let (local, http) = connect_commands(&gateway, &created.secret);
            out.push(
                v().gap(px(10.)).p(px(14.)).rounded(px(8.)).border_1().border_color(c(theme::ACCENT)).bg(c(theme::ATTENTION_BG))
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(format!("Key “{}” is ready. Copy the secret now: it is shown only once and Aurelius does not keep it.", created.key.name)))
                    .child(copy_row("app-key-copy-secret", created.secret.clone(), "Secret", cx))
                    .when_some(created.webhook_secret.clone(), |el, ws| el
                        .child(h().gap(px(6.)).items_center().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Webhook secret, also shown only once:").child(ui::info("webhook-secret-about", "Used to check the signature of each event.")))
                        .child(copy_row("app-key-copy-webhook-secret", ws, "Webhook secret", cx)))
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Add it to Claude Code on the server PC:"))
                    .child(copy_row("app-key-copy-local", local, "Command", cx))
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Or from another computer, through remote access:"))
                    .child(copy_row("app-key-copy-http", http, "Command", cx))
                    .child(ui::button("app-key-created-done", "I saved the secret", ButtonKind::Outline, None).w(px(160.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app_keys.created = None;
                        cx.notify();
                    })))
                    .into_any_element(),
            );
        }

        let (management, hooks) = match &self.connection { Connection::Online { server, .. } => (server.management_channel, server.webhooks), _ => (false, false) };
        if let Some(e) = self.app_keys.editor.as_ref() {
            out.push(
                v().gap(px(12.)).p(px(16.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER))
                    .child(div().font_family(ui::serif(cx)).text_size(px(18.)).child("New app key"))
                    // B1-2: what an app does runs on its maker's own Claude login.
                    .child(div().id("app-key-own-subscription").text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(crate::screens::account::OWN_SUBSCRIPTION))
                    .child(div().id("app-key-login").text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(crate::screens::account::APP_KEY_LOGIN))
                    .child(labelled_info("Name", "Tasks this app creates show this name.", ui::input(&e.name)))
                    .child(labelled(
                        "Scopes",
                        v().gap(px(6.)).children(AppScope::ALL.into_iter().enumerate().map(|(i, scope)| {
                            let locked = scope == AppScope::Approve && management && e.audience == AppAudience::Management;
                            let on = e.scopes.contains(&scope) && !locked;
                            h().gap(px(10.))
                                .child(chip(("app-key-scope", i), scope.as_str(), on).w(px(84.)).flex_none().when(locked, |el| el.opacity(0.4)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    if locked { return; }
                                    if let Some(e) = this.app_keys.editor.as_mut() {
                                        match e.scopes.iter().position(|s| *s == scope) {
                                            Some(at) => { e.scopes.remove(at); }
                                            None => e.scopes.push(scope),
                                        }
                                    }
                                    cx.notify();
                                })))
                                .child(ui::info(("app-key-scope-info", i), scope_help(scope)))
                        })),
                    ))
                    .when(management, |el| el.child(labelled(
                        "Audience",
                        v().gap(px(6.)).children([AppAudience::All, AppAudience::Management].into_iter().enumerate().map(|(i, audience)| {
                            h().gap(px(10.))
                                .child(chip(("app-key-audience", i), audience.as_str(), e.audience == audience).w(px(84.)).flex_none().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    if let Some(e) = this.app_keys.editor.as_mut() {
                                        e.audience = audience;
                                        if audience == AppAudience::Management {
                                            e.scopes.retain(|s| *s != AppScope::Approve);
                                        }
                                    }
                                    cx.notify();
                                })))
                                .child(ui::info(("app-key-audience-info", i), audience_help(audience)))
                        })),
                    )))
                    .when(hooks, |el| el.child(labelled_info("Webhook URL (optional)", "http or https. Aurelius posts this app's events there.", ui::input(&e.webhook))))
                    .child(
                        h().gap(px(12.)).items_start()
                            .child(div().flex_1().min_w_0().child(labelled_info("Tasks per hour", "Empty: no limit.", ui::input(&e.limit_hour))))
                            .child(div().flex_1().min_w_0().child(labelled_info("Tasks open at once", "Drafts are not counted. Empty: no limit.", ui::input(&e.limit_open)))),
                    )
                    .when_some(e.error.clone(), |el, why| el.child(notice(why, true)))
                    .child(
                        h().gap(px(8.)).justify_end()
                            .child(ui::button("app-key-cancel", "Cancel", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.app_keys.editor = None;
                                cx.notify();
                            })))
                            .child(ui::button("app-key-save", "Create key", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.app_key_save(cx)))),
                    )
                    .into_any_element(),
            );
        }

        if self.app_keys.loaded && self.app_keys.keys.is_empty() {
            out.push(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("No app keys yet.").into_any_element());
        }
        let confirm = self.app_keys.confirm_revoke;
        out.extend(self.app_keys.keys.iter().enumerate().map(|(i, k)| {
            let kid = k.id;
            let revoked = k.revoked_at_ms.is_some();
            let limit = |l: Option<u32>| l.map_or("no limit".to_string(), |n| n.to_string());
            v().id(("app-key", i)).gap(px(6.)).px(px(14.)).py(px(11.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER))
                .child(
                    h().gap(px(10.))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(14.)).text_color(c(if revoked { theme::MUTED } else { theme::TEXT })).child(k.name.clone()))
                        .child(div().text_size(px(11.5)).text_color(c(if revoked { theme::NEGATIVE } else { theme::MUTED })).child(match (k.revoked_at_ms, k.last_used_ms) {
                            (Some(at), _) => format!("Revoked {}", ui::fmt_when(at)),
                            (None, Some(at)) => format!("Last used {}", ui::fmt_when(at)),
                            (None, None) => "Never used".to_string(),
                        })),
                )
                .child(
                    h().gap(px(18.)).flex_wrap().text_size(px(12.)).text_color(c(theme::TEXT_2))
                        .child(format!("Scopes: {}", scopes_text(&k.scopes)))
                        .when(management, |el| el.child(format!("Audience: {}", k.audience.as_str())))
                        .when(hooks, |el| el.child(match &k.webhook_url {
                            Some(url) => format!("Webhook: {url}"),
                            None => "Webhook: none".to_string(),
                        }))
                        .child(format!("Tasks this hour: {}", usage_text(k.tasks_last_hour, k.limit_per_hour)))
                        .child(format!("Open now: {}", usage_text(k.tasks_open, k.limit_concurrent)))
                        .child(format!("Limits: {}/hour, {} at once", limit(k.limit_per_hour), limit(k.limit_concurrent)))
                        .child(format!("Made {}", ui::fmt_when(k.created_at_ms))),
                )
                .when(!revoked, |el| {
                    el.child(h().gap(px(8.)).map(|row| if confirm == Some(kid) {
                        row.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("Revoke for good? The app stops at once, also in open sessions; its tasks stay."))
                            .child(ui::button(("app-key-revoke-yes", i), "Revoke", ButtonKind::Primary, None).h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.app_keys_send(AppKeyCall::Revoke(kid));
                                cx.notify();
                            })))
                            .child(ui::button(("app-key-revoke-no", i), "Keep", ButtonKind::Outline, None).h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.app_keys.confirm_revoke = None;
                                cx.notify();
                            })))
                    } else {
                        row.child(ui::button(("app-key-revoke", i), "Revoke", ButtonKind::Outline, None).h(px(26.)).text_color(c(theme::NEGATIVE)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.app_keys.confirm_revoke = Some(kid);
                            cx.notify();
                        })))
                    }))
                })
                .into_any_element()
        }));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{audience_help, connect_commands, scopes_text, webhook_field};
    use aurelius_domain::app_key::AppScope;

    #[test]
    fn run_hides_the_draft_it_includes() {
        assert_eq!(scopes_text(&[AppScope::Read, AppScope::Draft, AppScope::Run]), "read, run");
        assert_eq!(scopes_text(&[AppScope::Draft, AppScope::Approve]), "draft, approve");
    }

    #[test]
    fn the_commands_carry_the_secret_and_the_mcp_path() {
        let (local, http) = connect_commands("https://pc.tail.ts.net:8443/", "amk_x");
        assert!(local.contains("AURELIUS_MCP_KEY=amk_x") && local.ends_with("-- aurelius-mcp"));
        assert!(http.contains("https://pc.tail.ts.net:8443/v1/mcp ") && http.contains("Bearer amk_x"));
    }

    #[test]
    fn the_webhook_field_is_empty_or_http() {
        assert_eq!(webhook_field("  "), Ok(None));
        assert_eq!(webhook_field(" https://h.example/x ").unwrap().as_deref(), Some("https://h.example/x"));
        assert!(webhook_field("ftp://h").is_err() && webhook_field("example.com").is_err() && webhook_field("http://a b").is_err());
        assert!(audience_help(aurelius_domain::app_key::AppAudience::Management).contains("no approve"));
    }
}

#[cfg(test)]
mod helper_tests {
    use super::{parse_limit, usage_text};

    #[test]
    fn limits_are_empty_or_at_least_one() {
        assert_eq!(parse_limit("  ", "x"), Ok(None));
        assert_eq!(parse_limit("25", "x"), Ok(Some(25)));
        assert!(parse_limit("0", "x").is_err());
        assert!(parse_limit("-3", "x").is_err());
        assert!(parse_limit("many", "x").is_err());
    }

    #[test]
    fn usage_shows_the_limit_or_none() {
        assert_eq!(usage_text(3, Some(10)), "3 of 10");
        assert_eq!(usage_text(3, None), "3 · no limit");
    }
}
