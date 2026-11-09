// What the chat popout lists and where it stays out of the way. Pure, so it is tested without a screen.

import type { ChannelView, EmployeeChatView, EmployeeSummary, TicketView } from "@aurelius/protocol";
import { bareName, sections } from "./chat-logic";

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

export interface DockTicket {
  conversation: string;
  ticketId: string;
  title: string;
  /** "Doing · Pixel". */
  line: string;
  unread: number;
  mentions: number;
  lastMs: number | null;
}

const OPEN_TICKETS_SHOWN = 12;

/** The open tickets to talk about, the latest first; all that match when searching. */
export function dockTickets(
  channels: ChannelView[],
  tickets: Pick<TicketView, "id" | "title" | "owner_name">[],
  search: string,
): DockTicket[] {
  const wanted = search.trim().toLowerCase();
  const all = sections(channels).tickets.flatMap((c): DockTicket[] => {
    if (!c.ticket_id) return [];
    const ticket = tickets.find((t) => t.id === c.ticket_id);
    const number = c.ticket_number == null ? "" : `#${c.ticket_number} `;
    const column = c.ticket_column
      ? c.ticket_column.charAt(0).toUpperCase() + c.ticket_column.slice(1).replace("_", " ")
      : "";
    return [
      {
        conversation: c.conversation_id,
        ticketId: c.ticket_id,
        title: `${number}${ticket?.title ?? bareName(c)}`,
        line: [column, ticket?.owner_name].filter(Boolean).join(" · "),
        unread: c.unread,
        mentions: c.mentions,
        lastMs: c.last_activity_ms ?? null,
      },
    ];
  });
  return wanted === "" ? all.slice(0, OPEN_TICKETS_SHOWN) : all.filter((t) => t.title.toLowerCase().includes(wanted));
}

/** The general channel and the project channels, to open in the Team chat; all that match when searching. */
export function dockChannels(channels: ChannelView[], search: string): ChannelView[] {
  const wanted = search.trim().toLowerCase();
  const { general, projects } = sections(channels);
  const all = [...(general ? [general] : []), ...projects];
  return wanted === "" ? all : all.filter((c) => bareName(c).toLowerCase().includes(wanted));
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
  /** The conversation is this ticket's discussion. */
  ticket: string | null;
}

export function parseDock(raw: string | null): DockState {
  try {
    const value = JSON.parse(raw ?? "null") as Partial<DockState> | null;
    const text = (v: unknown) => (typeof v === "string" ? v : null);
    return { open: value?.open === true, conversation: text(value?.conversation), ticket: text(value?.ticket) };
  } catch {
    return { open: false, conversation: null, ticket: null };
  }
}
