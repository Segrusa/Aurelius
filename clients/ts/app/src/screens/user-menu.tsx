// The signed-in board member at the foot of the sidebar: their name and Claude plan, and a menu above it
// with Settings, Usage, Remote access and Sign out.

import { DropdownMenu } from "@astryxdesign/core/DropdownMenu";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useConnection, useLinkState } from "../connection";
import { keysFor, SHORTCUTS, showChord } from "../keys/keymap";
import { isMac } from "../keys/provider";
import { usePlatform } from "../platform";
import { Icon } from "../ui/icons";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { planLabel } from "./user-menu-logic";

const styles = stylex.create({
  trigger: {
    display: "flex",
    width: "100%",
    minWidth: 0,
    alignItems: "center",
    gap: 8,
    cursor: "pointer",
    borderWidth: 0,
    borderRadius: 6,
    paddingBlock: 4,
    paddingInline: 6,
    font: "inherit",
    fontSize: 13,
    textAlign: "start",
    color: "var(--color-text-primary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  // The rail has room for the mark only.
  railTrigger: { width: "auto", paddingInline: 4 },
  name: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: 500 },
  plan: { flexShrink: 0, color: "var(--precision-text3)" },
  chevron: { display: "flex", marginInlineStart: "auto", color: "var(--precision-text3)" },
});

/** How the Settings key is shown here ("Ctrl ,", "⌘,"), from the keymap. */
function useSettingsKey(): string | null {
  const { kind } = usePlatform();
  const keys = SHORTCUTS.find((s) => s.action === "OpenSettings")?.keys ?? [];
  const key = keysFor(keys, kind)[0];
  return key ? showChord(key, isMac) : null;
}

export function UserMenu({ collapsed }: { collapsed: boolean }) {
  const navigate = useNavigate();
  const connection = useConnection();
  const { user, claudeLogin } = useLinkState();
  const settingsKey = useSettingsKey();
  if (!user) return null;
  const plan = planLabel(claudeLogin?.plan);
  const go = (to: string) => () => void navigate({ to });
  return (
    <DropdownMenu
      placement="above"
      alignment="start"
      menuWidth={240}
      menuMaxHeight={400}
      renderTrigger={(props) => (
        <button
          type="button"
          {...props}
          aria-label={`${user.username}: settings and account`}
          title={collapsed ? user.username : undefined}
          {...stylex.props(styles.trigger, collapsed && styles.railTrigger)}
        >
          <Mark letter={initial(user.username)} color={markColor(user.id)} size={22} radius={11} />
          {collapsed ? null : (
            <>
              <span {...stylex.props(styles.name)}>{user.username}</span>
              {plan ? <span {...stylex.props(styles.plan)}>{`· ${plan}`}</span> : null}
              <span {...stylex.props(styles.chevron)}>
                <Icon name="chevronDown" size={12} />
              </span>
            </>
          )}
        </button>
      )}
      items={[
        {
          // Who is signed in, as the menu's quiet first line: the Claude login's email when known.
          type: "section",
          title: claudeLogin?.email ?? user.username,
          items: [
            {
              label: "Settings",
              icon: <Icon name="settings" />,
              onClick: go("/settings"),
              ...(settingsKey ? { endContent: <KeyHint>{settingsKey}</KeyHint> } : {}),
            },
            { label: "Usage", icon: <Icon name="usage" />, onClick: go("/usage") },
            { label: "Remote access", icon: <Icon name="devices" />, onClick: go("/settings/remote") },
          ],
        },
        { type: "divider" },
        { label: "Sign out", icon: <Icon name="signOut" />, onClick: () => void connection.signOut() },
      ]}
    />
  );
}
