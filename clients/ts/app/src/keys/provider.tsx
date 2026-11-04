// One listener for the whole app. Screens say what they handle and which context is open; which key
// means what stays in the table.

import { createContext, type ReactNode, useContext, useEffect, useMemo, useRef } from "react";
import { usePlatform } from "../platform";
import { actionsFor, chordOf, type KeyState } from "./keymap";

type Handler = () => void;

interface Keys {
  stack: KeyState["stack"];
  pushContext(name: string, modal: boolean): () => void;
  addHandler(action: string, handler: { current: Handler }): () => void;
  has(action: string): boolean;
  run(action: string): void;
}

const KeysContext = createContext<Keys | null>(null);

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);

export function KeysProvider({ children }: { children: ReactNode }) {
  const { kind } = usePlatform();
  const keys = useMemo<Keys>(() => {
    const stack: KeyState["stack"] = [
      { name: "Aurelius", modal: false },
      { name: "Workspace", modal: false },
    ];
    const handlers = new Map<string, { current: Handler }[]>();
    const list = (action: string) => handlers.get(action) ?? [];
    return {
      stack,
      pushContext(name, modal) {
        const entry = { name, modal };
        stack.push(entry);
        return () => {
          stack.splice(stack.indexOf(entry), 1);
        };
      },
      addHandler(action, handler) {
        handlers.set(action, [...list(action), handler]);
        return () => {
          handlers.set(
            action,
            list(action).filter((h) => h !== handler),
          );
        };
      },
      has: (action) => list(action).length > 0,
      run: (action) => list(action).at(-1)?.current(),
    };
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      const chord = chordOf(event, isMac);
      if (chord === null) return;
      const target = event.target instanceof Element ? event.target : null;
      const typing = Boolean(target?.closest("input, textarea, select, [contenteditable=true]"));
      const onControl = Boolean(target?.closest("button, a, [role=menuitem]"));
      const action = actionsFor(chord, { stack: keys.stack, typing, onControl, kind }).find((a) => keys.has(a));
      if (action === undefined) return;
      event.preventDefault();
      keys.run(action);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keys, kind]);

  return <KeysContext.Provider value={keys}>{children}</KeysContext.Provider>;
}

export function useKeys(): Keys {
  const keys = useContext(KeysContext);
  if (keys === null) throw new Error("No keys.");
  return keys;
}

/** While mounted, `action` does this. The latest screen to ask for an action gets it. */
export function useAction(action: string, handler: Handler, enabled = true): void {
  const keys = useKeys();
  const latest = useRef(handler);
  latest.current = handler;
  useEffect(() => (enabled ? keys.addHandler(action, latest) : undefined), [keys, action, enabled]);
}

/** While mounted, this context is open. A modal one shuts out everything behind it. */
export function useKeyContext(name: string, modal = false): void {
  const keys = useKeys();
  useEffect(() => keys.pushContext(name, modal), [keys, name, modal]);
}
