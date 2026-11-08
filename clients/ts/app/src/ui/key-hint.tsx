// A shortcut shown next to its action, from what keymap.ts shows ("⌘↵", "Ctrl ↵") or a plain key.

import { Kbd as AstryxKbd } from "@astryxdesign/core/Kbd";
import * as stylex from "@stylexjs/stylex";

const GLYPH_KEYS: Record<string, string> = {
  "⌘": "mod",
  "⌃": "ctrl",
  "⌥": "alt",
  "⇧": "shift",
  "↵": "enter",
  "↑": "up",
  "↓": "down",
  "←": "left",
  "→": "right",
};

/** What the design shows ("⌘↵", "Ctrl ↵", "esc") as Astryx's "mod+enter". */
export function keysOf(shown: string): string {
  const words = shown
    .replace(/[⌘⌃⌥⇧↵↑↓←→]/g, (glyph) => ` ${GLYPH_KEYS[glyph]} `)
    .trim()
    .split(/\s+/)
    .map((word) => (word === "Ctrl" ? "mod" : word.toLowerCase()));
  return words.join("+");
}

const kbdStyles = stylex.create({
  onAccent: { "--color-text-secondary": "#fff", "--precision-kbd": "rgb(255 255 255 / 0.18)" },
});

export function KeyHint({ children, onAccent = false }: { children: string; onAccent?: boolean }) {
  return <AstryxKbd keys={keysOf(children)} {...(onAccent ? stylex.props(kbdStyles.onAccent) : {})} />;
}
