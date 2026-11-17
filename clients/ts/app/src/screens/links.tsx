// Following an `aurelius://` link, from the operating system or from a message.

import { expectBody } from "@aurelius/client";
import { useNavigate } from "@tanstack/react-router";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { toast } from "../ui/toast";
import { type AppLink, INVALID_LINK, linkedTicket, NOT_PAIRED, OLD_SERVER, parseLink } from "./links-logic";
import { Confirm } from "./org-kit";
import { ticketPlace } from "./ticket-session-logic";

const FollowContext = createContext<((url: string) => void) | null>(null);

/** Opens a link; null outside the signed-in app, where a link stays text. */
export const useFollowLink = () => useContext(FollowContext);

export function LinkFollower({ children }: { children: ReactNode }) {
  const platform = usePlatform();
  const connection = useConnection();
  const navigate = useNavigate();
  const { hostId, features } = useLinkState();
  const [elsewhere, setElsewhere] = useState<{ url: string; hostId: string; name: string } | null>(null);

  const go = useCallback(
    async (link: AppLink) => {
      if (link.kind === "ticket") {
        // A number since ticket #70; an older link has the id, which the Board's redirect resolves.
        const which = linkedTicket(link.id);
        void navigate(ticketPlace("number" in which ? { id: "", number: which.number } : { id: which.id }));
        return;
      }
      if (!features.includes("message_locate")) {
        toast(OLD_SERVER);
        return;
      }
      try {
        const body = await connection.call("message.locate", { message_id: link.id });
        const place = expectBody(body, "message_location").value;
        void navigate({
          to: "/chat",
          search: {
            channel: place.conversation_id,
            message: place.message_id,
            ...(place.thread_root ? { thread: place.thread_root } : {}),
          },
        });
      } catch (error) {
        toast(error instanceof Error ? error.message : "This message was not found.");
      }
    },
    [connection, features, navigate],
  );

  const follow = useCallback(
    (url: string) => {
      const link = parseLink(url);
      if (link === null) {
        toast(INVALID_LINK);
        return;
      }
      if (link.host === hostId) {
        void go(link);
        return;
      }
      if (platform.servers === null) {
        toast(NOT_PAIRED);
        return;
      }
      void platform.servers.list().then((choices) => {
        const known = choices.servers.find((s) => s.hostId === link.host);
        if (known) setElsewhere({ url, hostId: known.hostId, name: known.hostName });
        else toast(NOT_PAIRED);
      });
    },
    [go, hostId, platform.servers],
  );

  // The latest `follow` for the listener, which the platform keeps once.
  const current = useRef(follow);
  current.current = follow;
  useEffect(() => {
    const { osLinks } = platform;
    if (osLinks === null) return;
    const take = () => {
      void osLinks.take().then((url) => {
        if (url !== null) current.current(url);
      });
    };
    osLinks.listen(take);
    take();
  }, [platform]);

  return (
    <FollowContext.Provider value={follow}>
      {children}
      {elsewhere ? (
        <Confirm
          title={`Switch to ${elsewhere.name}?`}
          body="This link is on another server. The app starts over on that server, then opens it."
          action="Switch"
          onClose={() => setElsewhere(null)}
          onConfirm={() => {
            void (async () => {
              await platform.osLinks?.hold(elsewhere.url);
              await platform.servers?.use(elsewhere.hostId);
            })();
          }}
        />
      ) : null}
    </FollowContext.Provider>
  );
}
