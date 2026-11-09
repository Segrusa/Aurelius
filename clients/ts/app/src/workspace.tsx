// The current workspace: one piece of state, kept per device and per server. A project window is
// its project's, whatever the main window has.

import type { ProjectSummary } from "@aurelius/protocol";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { useLinkState } from "./connection";
import { useProjects } from "./data";
import { usePlatform } from "./platform";
import {
  resolveWorkspace,
  storedWorkspace,
  WORKSPACE_FEATURE,
  type Workspace,
  workspaceKey,
} from "./screens/workspace-logic";

export interface WorkspaceState {
  /** A project id; null is All projects. */
  id: Workspace;
  project: ProjectSummary | null;
  /** The server can narrow what has no project on its rows: otherwise there are no workspaces. */
  available: boolean;
  /** A project window stays on its project. */
  locked: boolean;
  choose(next: Workspace): void;
}

const WorkspaceContext = createContext<WorkspaceState>({
  id: null,
  project: null,
  available: false,
  locked: false,
  choose: () => {},
});

export const useWorkspace = () => useContext(WorkspaceContext);

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const { storage, windowProject } = usePlatform();
  const { hostId, features } = useLinkState();
  const projects = useProjects();
  const available = features.includes(WORKSPACE_FEATURE);
  const key = workspaceKey(hostId);
  const [chosen, setChosen] = useState(() => ({ key, id: storedWorkspace(storage.get(key)) }));
  if (chosen.key !== key) setChosen({ key, id: storedWorkspace(storage.get(key)) });

  const stored = chosen.key === key ? chosen.id : storedWorkspace(storage.get(key));
  const resolved = resolveWorkspace(stored, projects.data);
  const locked = windowProject !== null;
  const id = !available ? null : locked ? windowProject : resolved;

  // An archived or removed project is forgotten, so an open of the app later does not need the list.
  useEffect(() => {
    if (!locked && stored !== null && resolved === null) storage.set(key, "");
  }, [locked, stored, resolved, storage, key]);

  const choose = useCallback(
    (next: Workspace) => {
      if (locked) return;
      storage.set(key, next ?? "");
      setChosen({ key, id: next });
    },
    [locked, storage, key],
  );

  const value = useMemo(
    () => ({ id, project: projects.data?.find((p) => p.id === id) ?? null, available, locked, choose }),
    [id, projects.data, available, locked, choose],
  );
  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}
