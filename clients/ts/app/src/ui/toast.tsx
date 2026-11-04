import { useSyncExternalStore } from "react";

interface Toast {
  id: number;
  text: string;
}

const LIFE_MS = 5000;
const MAX = 5;
let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const changed = () => {
  for (const listener of listeners) listener();
};

/** A short message that leaves by itself. */
export function toast(text: string): void {
  const id = nextId++;
  toasts = [...toasts, { id, text }].slice(-MAX);
  changed();
  setTimeout(() => {
    toasts = toasts.filter((t) => t.id !== id);
    changed();
  }, LIFE_MS);
}

export function Toasts() {
  const shown = useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => toasts,
  );
  return (
    <section
      aria-live="polite"
      aria-label="Messages"
      className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-[320px] flex-col gap-2"
    >
      {shown.map((t) => (
        <div key={t.id} className="enter rounded-[8px] border border-line bg-pop px-3 py-[10px] shadow-pop">
          {t.text}
        </div>
      ))}
    </section>
  );
}
