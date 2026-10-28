//! 'Always allow' for MCP tools: the button next to Approve/Reject wherever an
//! approval is answered, and Settings → MCP & tools. Servers without
//! the feature send no `always_allow_eligible` and don't know the list method,
//! so everything here stays hidden there.

use crate::app::AppView;
use crate::backend::{self, Connection};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h};
use aurelius_protocol::ApprovalView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub const TIP: &str = "Allows this tool in every project without asking again (Plan mode still blocks)";

/// Settings → MCP & tools → Always allowed tools.
#[derive(Default)]
pub struct AlwaysAllowState {
    /// The tools, or `None` while not loaded or when the server lacks the list.
    pub tools: Option<Vec<String>>,
    /// The tool being taken off the list.
    pub removing: Option<String>,
    /// The last failed Remove.
    pub error: Option<String>,
}

/// `mcp__github__create_issue` → `github › create_issue`; anything else as it is.
pub fn readable(tool: &str) -> String {
    let bare = tool.strip_prefix("claude.").unwrap_or(tool);
    match bare.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")) {
        Some((server, name)) if !server.is_empty() && !name.is_empty() => format!("{server} › {name}"),
        _ => bare.to_owned(),
    }
}

/// Whether to offer 'Always allow' for `a`: the server marks it eligible and this connection may approve.
pub fn offered(a: &ApprovalView, may_approve: bool) -> bool {
    a.always_allow_eligible && may_approve
}

impl AppView {
    /// This connection has the Approve right.
    pub fn may_approve(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.approve)
    }

    /// Approve `a` and allow its tool from now on; the server approves the tool's other pending asks too.
    pub fn always_allow(&mut self, a: &ApprovalView, cx: &mut Context<Self>) {
        if !offered(a, self.may_approve()) {
            return;
        }
        let _ = self.requests_handle().send(backend::Request::DecideApproval { approval_id: a.id, action_digest: a.action_digest.clone(), approve: true, always_allow: true });
        self.notify_user(format!("{} is now always allowed.", readable(&a.tool)), false, cx);
        cx.notify();
    }

    /// The 'Always allow' button for `a`, when offered. `key` keeps ids apart when the approval shows twice.
    pub fn always_allow_button(&self, a: &ApprovalView, key: &str, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !offered(a, self.may_approve()) {
            return None;
        }
        let approval = a.clone();
        Some(
            ui::button(SharedString::from(format!("always-{key}-{}", a.id)), "Always allow", ButtonKind::Outline, None)
                .tooltip(ui::tip(TIP, None))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.always_allow(&approval, cx))),
        )
    }

    pub fn remove_always_allowed(&mut self, tool: &str, cx: &mut Context<Self>) {
        if !self.may_approve() || self.always_allow.removing.is_some() {
            return;
        }
        self.always_allow.removing = Some(tool.to_owned());
        self.always_allow.error = None;
        let _ = self.requests_handle().send(backend::Request::McpAlwaysAllowRemove(tool.to_owned()));
        cx.notify();
    }

    pub fn always_allow_loaded(&mut self, tools: Option<Vec<String>>) {
        self.always_allow.tools = tools;
        self.always_allow.removing = None;
    }

    pub fn always_allow_remove_failed(&mut self, why: String) {
        self.always_allow.removing = None;
        self.always_allow.error = Some(why);
    }

    /// Settings → Always allowed tools, under MCP servers; nothing on servers without the list.
    pub fn render_always_allowed(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(tools) = &self.always_allow.tools else { return Vec::new() };
        let may = self.may_approve();
        let mut out = vec![
            div().pt(px(6.)).text_size(px(13.)).text_color(c(theme::TEXT))
                .child(ui::label_with_info("Always allowed tools", "These MCP tools run without asking, in every project. Plan mode still blocks them."))
                .into_any_element(),
        ];
        if tools.is_empty() {
            out.push(div().text_size(px(13.)).text_color(c(theme::MUTED))
                .child("No tools yet. Choose 'Always allow' on an MCP tool's approval to add one.").into_any_element());
        }
        out.extend(tools.iter().enumerate().map(|(i, tool)| {
            let removing = self.always_allow.removing.as_deref() == Some(tool.as_str());
            let t = tool.clone();
            h().gap(px(12.)).items_center().text_size(px(13.))
                .child(div().flex_1().min_w_0().truncate().text_color(c(theme::TEXT)).child(readable(tool)))
                .map(|el| if removing {
                    el.child(div().text_color(c(theme::MUTED)).child("Removing…"))
                } else {
                    el.child(ui::button(("always-remove", i), "Remove", ButtonKind::Outline, None).h(px(28.)).text_color(c(theme::NEGATIVE))
                        .when(!may, |el| el.opacity(0.45))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_always_allowed(&t, cx))))
                })
                .into_any_element()
        }));
        if !may && !tools.is_empty() {
            out.push(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("You need the Approve right on this server to remove tools.").into_any_element());
        }
        if let Some(why) = &self.always_allow.error {
            out.push(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(why.clone()).into_any_element());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{offered, readable};

    #[test]
    fn tool_names_read_as_server_and_tool() {
        assert_eq!(readable("mcp__github__create_issue"), "github › create_issue");
        assert_eq!(readable("claude.mcp__claude-in-chrome__navigate"), "claude-in-chrome › navigate");
        assert_eq!(readable("mcp__a__b__c"), "a › b__c");
        assert_eq!(readable("claude.Bash"), "Bash");
        assert_eq!(readable("mcp__x"), "mcp__x");
    }

    #[test]
    fn offered_only_when_eligible_and_allowed_to_approve() {
        let mut a: aurelius_protocol::ApprovalView = serde_json::from_value(serde_json::json!({
            "id": aurelius_domain::ApprovalId::new(), "status": "pending", "tool": "claude.mcp__github__create_issue",
            "effect": "external", "arguments": {}, "resources": [], "account": "me", "reasons": [], "action_digest": "d",
            "requested_at_ms": 0, "expires_at_ms": 0, "run_id": aurelius_domain::RunId::new(), "conversation_id": null,
            "requester": "Alex", "task_id": null
        })).expect("an approval from a server without the field");
        assert!(!a.always_allow_eligible, "older servers leave it off");
        assert!(!offered(&a, true));
        a.always_allow_eligible = true;
        assert!(offered(&a, true));
        assert!(!offered(&a, false), "needs the Approve right");
    }
}
