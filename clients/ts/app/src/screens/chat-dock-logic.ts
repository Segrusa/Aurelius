// What the chat popout lists and where it stays out of the way. Pure, so it is tested without a screen.

import type { EmployeeChatView, EmployeeSummary } from "@aurelius/protocol";

/** Where a chat already fills the screen: a popout next to it adds nothing. */
export function dockAway(pathname: string): boolean {
  return pathname === "/chat" || pathname.startsWith("/chat/") || pathname.startsWith("/talk/");
}

/** Where a message box sits in the corner the launcher would cover. */
export function launcherAway(pathname: string, ticketOpen: boolean): boolean {
  return ticketOpen || /^\/tasks\/[^/]+/.test(pathname);
}

export interface DockChats {
  chats: EmployeeChatView[];
  /** Active employees without a chat yet. */
  others: EmployeeSummary[];
  unread: number;
}

export function dockChats(chats: EmployeeChatView[], employees: EmployeeSummary[], search: string): DockChats {
  const wanted = search.trim().toLowerCase();
  const fits = (name: string) => wanted === "" || name.toLowerCase().includes(wanted);
  const have = new Set(chats.map((c) => c.employee_id));
  return {
    chats: chats.filter((c) => fits(c.employee_name)),
    others: employees.filter((e) => e.status === "active" && !have.has(e.id) && fits(e.name)),
    unread: chats.reduce((sum, c) => sum + c.unread, 0),
  };
}

/** "now", "5m", "3h", "2d", "1w": the age of a chat. */
export function chatAge(atMs: number, nowMs: number): string {
  const minutes = Math.max(0, Math.floor((nowMs - atMs) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return days < 7 ? `${days}d` : `${Math.floor(days / 7)}w`;
}

export interface DockState {
  open: boolean;
  conversation: string | null;
}

export function parseDock(raw: string | null): DockState {
  try {
    const value = JSON.parse(raw ?? "null") as Partial<DockState> | null;
    return {
      open: value?.open === true,
      conversation: typeof value?.conversation === "string" ? value.conversation : null,
    };
  } catch {
    return { open: false, conversation: null };
  }
}
