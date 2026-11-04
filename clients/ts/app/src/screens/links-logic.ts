// `aurelius://<host_id>/ticket/<ticket_id>` and `aurelius://<host_id>/message/<message_id>`: they
// hold no secret, and only say where something is (clients/desktop/src/screens/links.rs).

export interface AppLink {
  host: string;
  kind: "ticket" | "message";
  id: string;
}

const SCHEME = "aurelius://";
const ID = /^[A-Za-z0-9-]+$/;

export const INVALID_LINK = "This isn't a valid Aurelius link.";
export const OLD_SERVER = "This server can't open links yet. Update the server.";
export const NOT_PAIRED = "This link is for a server this device isn't paired with.";

export function parseLink(text: string): AppLink | null {
  if (!text.startsWith(SCHEME)) return null;
  const parts = text.slice(SCHEME.length).split("/");
  if (parts.length !== 3) return null;
  const [host, kind, id] = parts as [string, string, string];
  if (!ID.test(host) || !ID.test(id)) return null;
  return kind === "ticket" || kind === "message" ? { host, kind, id } : null;
}

export const ticketLink = (host: string, ticket: string) => `${SCHEME}${host}/ticket/${ticket}`;
export const messageLink = (host: string, message: string) => `${SCHEME}${host}/message/${message}`;
