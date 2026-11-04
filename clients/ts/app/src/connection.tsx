import { Connection, type LinkState } from "@aurelius/client";
import { createContext, type ReactNode, useContext, useEffect, useState, useSyncExternalStore } from "react";
import type { Platform } from "./platform";

const ConnectionContext = createContext<Connection | null>(null);

export function ConnectionProvider({
  platform,
  generation,
  children,
}: {
  platform: Platform;
  generation: number;
  children: ReactNode;
}) {
  const [connection, setConnection] = useState<Connection | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new generation is a new pairing, so a new connection.
  useEffect(() => {
    const next = new Connection({
      open: platform.openLink,
      sessions: platform.sessions,
      clientName: `aurelius-${platform.kind}`,
      clientVersion: platform.version,
      deviceName: platform.deviceName,
      ...(platform.target ? { expectedHostId: platform.target.hostId, environment: platform.target.environment } : {}),
    });
    next.start();
    setConnection(next);
    return () => next.dispose();
  }, [platform, generation]);
  if (connection === null) return null;
  return <ConnectionContext.Provider value={connection}>{children}</ConnectionContext.Provider>;
}

export function useConnection(): Connection {
  const connection = useContext(ConnectionContext);
  if (connection === null) throw new Error("No connection.");
  return connection;
}

export function useLinkState(): LinkState {
  const connection = useConnection();
  return useSyncExternalStore(connection.subscribe, connection.getState);
}
