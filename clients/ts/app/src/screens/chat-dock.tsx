// The chat popout: a corner of any screen to talk with an employee, without leaving what you are doing.

import { IconButton } from "@astryxdesign/core/IconButton";
import { TextInput } from "@astryxdesign/core/TextInput";
import type { EmployeeChatView, EmployeeSummary } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { type KeyboardEvent, useEffect, useMemo, useState } from "react";
import { useLinkState } from "../connection";
import { useEmployeeChats, useEmployees } from "../data";
import { usePlatform } from "../platform";
import { Icon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { chatAge, type DockState, dockAway, dockChats, launcherAway, parseDock } from "./chat-dock-logic";
import { EmployeeChat, useChatOpener } from "./employee-chat";

const DOCK_KEY = "aurelius.chat-dock";

const popIn = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(10px) scale(0.98)" },
  to: { opacity: 1, transform: "none" },
});

const styles = stylex.create({
  launcher: {
    position: "fixed",
    right: 20,
    bottom: 20,
    zIndex: 30,
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 46,
    height: 46,
    borderRadius: 15,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    padding: 0,
    cursor: "pointer",
    color: "var(--color-text-primary)",
    backgroundColor: { default: "var(--color-background-popover)", ":hover": "var(--color-neutral)" },
    boxShadow: "var(--shadow-high)",
    transitionProperty: "transform, background-color",
    transitionDuration: "140ms",
    transform: { default: "none", ":hover": "translateY(-2px)", ":active": "scale(0.96)" },
  },
  badge: {
    position: "absolute",
    top: -5,
    right: -5,
    minWidth: 18,
    height: 18,
    borderRadius: 9,
    paddingInline: 5,
    boxSizing: "border-box",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    fontSize: 11,
    fontWeight: 600,
    color: "var(--color-on-accent)",
    backgroundColor: "var(--color-accent)",
  },
  panel: {
    position: "fixed",
    right: 20,
    bottom: 20,
    zIndex: 30,
    display: "flex",
    flexDirection: "column",
    width: "min(400px, calc(100vw - 32px))",
    height: "min(640px, calc(100vh - 40px))",
    overflow: "hidden",
    borderRadius: 18,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-background-surface)",
    boxShadow: "var(--shadow-high)",
    transformOrigin: "bottom right",
    animationName: popIn,
    animationDuration: "170ms",
    animationTimingFunction: "cubic-bezier(0.2, 0.8, 0.2, 1)",
  },
  head: { display: "flex", flexShrink: 0, alignItems: "center", gap: 8, minHeight: 56, paddingInline: 12 },
  title: { margin: 0, flexGrow: 1, minWidth: 0, fontSize: 16, fontWeight: 600 },
  who: { display: "flex", flexDirection: "column", minWidth: 0, flexGrow: 1, flexBasis: 0 },
  whoName: { fontSize: 13, fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  whoRole: {
    fontSize: 12,
    color: "var(--precision-text3)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  search: { flexShrink: 0, paddingInline: 12, paddingBottom: 8 },
  list: { display: "flex", flexDirection: "column", minHeight: 0, flexGrow: 1, overflowY: "auto", paddingBottom: 8 },
  label: {
    margin: 0,
    paddingInline: 16,
    paddingTop: 12,
    paddingBottom: 4,
    fontSize: 12,
    fontWeight: 600,
    color: "var(--color-text-secondary)",
  },
  row: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    width: "100%",
    borderWidth: 0,
    paddingBlock: 9,
    paddingInline: 16,
    textAlign: "start",
    cursor: "pointer",
    color: "inherit",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  rowText: { display: "flex", flexDirection: "column", gap: 1, minWidth: 0, flexGrow: 1, flexBasis: 0 },
  rowTop: { display: "flex", alignItems: "baseline", gap: 8 },
  rowName: {
    minWidth: 0,
    flexGrow: 1,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontWeight: 500,
  },
  rowNameNew: { fontWeight: 700 },
  rowAge: { flexShrink: 0, fontSize: 12, color: "var(--precision-text3)" },
  rowLine: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  rowLineNew: { color: "var(--color-text-primary)", fontWeight: 600 },
  dot: { flexShrink: 0, width: 8, height: 8, borderRadius: "50%", backgroundColor: "var(--color-accent)" },
  nothing: { margin: 0, padding: 16, fontSize: 13, color: "var(--color-text-secondary)" },
  chat: { display: "flex", flexDirection: "column", minHeight: 0, flexGrow: 1, flexBasis: 0 },
});

const roleOf = (e: EmployeeSummary | undefined) => (e ? (e.title ?? e.role) : "");

function ChatLine({
  chat,
  role,
  now,
  onOpen,
}: {
  chat: EmployeeChatView;
  role: string;
  now: number;
  onOpen: () => void;
}) {
  const fresh = chat.unread > 0;
  return (
    <button type="button" onClick={onOpen} {...stylex.props(styles.row)}>
      <Mark letter={initial(chat.employee_name)} color={markColor(chat.employee_id)} size={38} radius={12} />
      <span {...stylex.props(styles.rowText)}>
        <span {...stylex.props(styles.rowTop)}>
          <span {...stylex.props(styles.rowName, fresh && styles.rowNameNew)}>{chat.employee_name}</span>
          <span {...stylex.props(styles.rowAge)}>{chatAge(chat.last_activity_ms, now)}</span>
        </span>
        <span {...stylex.props(styles.rowLine, fresh && styles.rowLineNew)}>{chat.preview ?? role}</span>
      </span>
      {fresh ? <span title={`${chat.unread} unread`} {...stylex.props(styles.dot)} /> : null}
    </button>
  );
}

function ChatList({ onPick, onCollapse }: { onPick: (conversation: string) => void; onCollapse: () => void }) {
  const navigate = useNavigate();
  const [search, setSearch] = useState("");
  const chats = useEmployeeChats();
  const employees = useEmployees();
  const open = useChatOpener(onPick);
  const list = useMemo(
    () => dockChats(chats.data ?? [], employees.data ?? [], search),
    [chats.data, employees.data, search],
  );
  const now = Date.now();
  const byId = (id: string) => employees.data?.find((e) => e.id === id);
  return (
    <>
      <div {...stylex.props(styles.head)}>
        <h2 {...stylex.props(styles.title)}>Chat</h2>
        <IconButton
          label="Open Team chat"
          tooltip="Open Team chat"
          variant="ghost"
          size="sm"
          icon={<Icon name="expand" size={15} />}
          onClick={() => {
            onCollapse();
            void navigate({ to: "/chat" });
          }}
        />
        <IconButton
          label="Collapse chat"
          tooltip="Collapse"
          variant="ghost"
          size="sm"
          icon={<Icon name="chevronDown" size={15} />}
          onClick={onCollapse}
        />
      </div>
      <div {...stylex.props(styles.search)}>
        <TextInput
          label="Search chats"
          isLabelHidden
          placeholder="Search"
          width="100%"
          value={search}
          onChange={setSearch}
        />
      </div>
      <div {...stylex.props(styles.list)}>
        {list.chats.map((c) => (
          <ChatLine
            key={c.conversation_id}
            chat={c}
            role={roleOf(byId(c.employee_id))}
            now={now}
            onOpen={() => onPick(c.conversation_id)}
          />
        ))}
        {list.others.length > 0 ? <p {...stylex.props(styles.label)}>Start a chat</p> : null}
        {list.others.map((e) => (
          <button key={e.id} type="button" onClick={() => open(e.id)} {...stylex.props(styles.row)}>
            <Mark letter={initial(e.name)} color={markColor(e.id)} size={38} radius={12} />
            <span {...stylex.props(styles.rowText)}>
              <span {...stylex.props(styles.rowName)}>{e.name}</span>
              <span {...stylex.props(styles.rowLine)}>{roleOf(e)}</span>
            </span>
          </button>
        ))}
        {list.chats.length === 0 && list.others.length === 0 ? (
          <p {...stylex.props(styles.nothing)}>Nobody by that name.</p>
        ) : null}
      </div>
    </>
  );
}

function OneChat({
  conversation,
  onBack,
  onCollapse,
}: {
  conversation: string;
  onBack: () => void;
  onCollapse: () => void;
}) {
  const navigate = useNavigate();
  const chats = useEmployeeChats();
  const employees = useEmployees();
  const chat = chats.data?.find((c) => c.conversation_id === conversation);
  const employee = employees.data?.find((e) => e.id === chat?.employee_id);
  const name = chat?.employee_name ?? employee?.name ?? "Chat";
  return (
    <>
      <div {...stylex.props(styles.head)}>
        <IconButton
          label="All chats"
          tooltip="All chats"
          variant="ghost"
          size="sm"
          icon={<Icon name="back" size={15} />}
          onClick={onBack}
        />
        <Mark letter={initial(name)} color={markColor(chat?.employee_id ?? name)} size={30} radius={10} />
        <div {...stylex.props(styles.who)}>
          <span {...stylex.props(styles.whoName)}>{name}</span>
          <span {...stylex.props(styles.whoRole)}>{roleOf(employee)}</span>
        </div>
        <IconButton
          label="Open full screen"
          tooltip="Open full screen"
          variant="ghost"
          size="sm"
          icon={<Icon name="expand" size={15} />}
          onClick={() => {
            onCollapse();
            void navigate({ to: "/talk/$conversationId", params: { conversationId: conversation } });
          }}
        />
        <IconButton
          label="Collapse chat"
          tooltip="Collapse"
          variant="ghost"
          size="sm"
          icon={<Icon name="chevronDown" size={15} />}
          onClick={onCollapse}
        />
      </div>
      <div {...stylex.props(styles.chat)}>
        <EmployeeChat key={conversation} conversationId={conversation} compact />
      </div>
    </>
  );
}

export function ChatDock() {
  const { features } = useLinkState();
  const { storage } = usePlatform();
  const location = useRouterState({ select: (s) => s.location });
  const [state, setState] = useState<DockState>(() => parseDock(storage.get(DOCK_KEY)));
  const chats = useEmployeeChats();
  useEffect(() => {
    storage.set(DOCK_KEY, JSON.stringify(state));
  }, [state, storage]);
  const unread = useMemo(() => (chats.data ?? []).reduce((sum, c) => sum + c.unread, 0), [chats.data]);
  if (!features.includes("employee_chats") || dockAway(location.pathname)) return null;
  const collapse = () => setState((s) => ({ ...s, open: false }));
  if (!state.open) {
    if (launcherAway(location.pathname, Boolean((location.search as { ticket?: unknown }).ticket))) return null;
    return (
      <button
        type="button"
        aria-label={unread > 0 ? `Open chat, ${unread} unread` : "Open chat"}
        onClick={() => setState((s) => ({ ...s, open: true }))}
        {...stylex.props(styles.launcher)}
      >
        <Icon name="chat" size={20} />
        {unread > 0 ? <span {...stylex.props(styles.badge)}>{unread > 99 ? "99+" : unread}</span> : null}
      </button>
    );
  }
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.stopPropagation();
    collapse();
  };
  return (
    <section aria-label="Chat popout" onKeyDown={onKeyDown} {...stylex.props(styles.panel)}>
      {state.conversation ? (
        <OneChat
          conversation={state.conversation}
          onBack={() => setState((s) => ({ ...s, conversation: null }))}
          onCollapse={collapse}
        />
      ) : (
        <ChatList onPick={(conversation) => setState((s) => ({ ...s, conversation }))} onCollapse={collapse} />
      )}
    </section>
  );
}
