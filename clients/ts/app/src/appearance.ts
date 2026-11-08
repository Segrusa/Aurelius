// System, light or dark, kept per device. Only `data-theme` changes, so nothing re-renders.

import { createContext, useContext, useSyncExternalStore } from "react";

export type Appearance = "system" | "light" | "dark";

const KEY = "appearance";

export function readAppearance(storage: { get(key: string): string | null }): Appearance {
  const value = storage.get(KEY);
  return value === "light" || value === "dark" ? value : "system";
}

export function applyAppearance(appearance: Appearance): () => void {
  const root = document.documentElement;
  if (appearance !== "system") {
    root.dataset.theme = appearance;
    return () => {};
  }
  const query = matchMedia("(prefers-color-scheme: light)");
  const apply = () => {
    root.dataset.theme = query.matches ? "light" : "dark";
  };
  apply();
  query.addEventListener("change", apply);
  return () => query.removeEventListener("change", apply);
}

export function saveAppearance(storage: { set(key: string, value: string): void }, appearance: Appearance): void {
  storage.set(KEY, appearance);
}

export const AppearanceContext = createContext<{ appearance: Appearance; choose(appearance: Appearance): void }>({
  appearance: "system",
  choose: () => {},
});

export function useAppearance() {
  return useContext(AppearanceContext);
}

const PREFERS_LIGHT = "(prefers-color-scheme: light)";

function subscribeToSystem(changed: () => void) {
  const query = matchMedia(PREFERS_LIGHT);
  query.addEventListener("change", changed);
  return () => query.removeEventListener("change", changed);
}

/** Light or dark right now. Astryx gets this and never "system": `data-theme` must hold the resolved value. */
export function useResolvedMode(appearance: Appearance): "light" | "dark" {
  const systemLight = useSyncExternalStore(subscribeToSystem, () => matchMedia(PREFERS_LIGHT).matches);
  if (appearance !== "system") return appearance;
  return systemLight ? "light" : "dark";
}
