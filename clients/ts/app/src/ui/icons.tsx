// The 16 px stroke icons of the design.

import * as stylex from "@stylexjs/stylex";
import type { ComponentType } from "react";

const styles = stylex.create({
  fixed: { flexShrink: 0 },
});

const NAV_PATHS = {
  inbox: "M2.5 9.5h3l1 2h3l1-2h3M4 3.5h8l1.5 6V13h-11V9.5z",
  mine: "M8 2.5a5.5 5.5 0 1 0 0 11a5.5 5.5 0 1 0 0-11zM8 5.5V8l1.8 1.2",
  board: "M2.5 3.5h3v9h-3zM6.5 3.5h3v6h-3zM10.5 3.5h3v7.5h-3z",
  tasks: "M3 3h10a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM5 6.5l2 1.5-2 1.5M8.5 10h2.5",
  chat: "M2.5 3.5h11v7.5H7l-3 2.5V11H2.5z",
  team: "M6 3.3a2.2 2.2 0 1 0 0 4.4a2.2 2.2 0 1 0 0-4.4zM2.5 13c.4-2.2 1.8-3.4 3.5-3.4s3.1 1.2 3.5 3.4M11.3 4.6a1.7 1.7 0 1 0 0 3.4a1.7 1.7 0 1 0 0-3.4zM10.6 9.6c1.6 0 2.6 1 2.9 3",
  settings:
    "M8 5.6a2.4 2.4 0 1 0 0 4.8a2.4 2.4 0 1 0 0-4.8zM8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2",
  back: "M9.5 3.5 5 8l4.5 4.5",
  clock: "M3 3.5h10v10H3zM3 6.5h10M5.5 2v3M10.5 2v3",
  note: "M4 2.5h8v11H4zM6 5.5h4M6 8h4M6 10.5h2.5",
  branch:
    "M4.5 5.2a1.6 1.6 0 1 0 0-3.2a1.6 1.6 0 0 0 0 3.2zM4.5 14a1.6 1.6 0 1 0 0-3.2a1.6 1.6 0 0 0 0 3.2zM11.5 7.2a1.6 1.6 0 1 0 0-3.2a1.6 1.6 0 0 0 0 3.2zM4.5 5.2v5.6M11.5 7.2c0 2.2-2.4 2.3-4.4 2.6c-1.2.2-2.6.5-2.6 1",
  smile:
    "M13.5 7.6V8A5.5 5.5 0 1 1 8 2.5h.4M5.7 9.5s.8 1.3 2.3 1.3 2.3-1.3 2.3-1.3M6 6.5h.01M10 6.5h.01M12 1.8v3.4M10.3 3.5h3.4",
  reply: "M6.2 3.8 2.5 7.5l3.7 3.7M2.5 7.5h6.7a4.3 4.3 0 0 1 4.3 4.3v.9",
  copy: "M6.5 5.5h6a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-6a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1zM3.5 10.5a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1",
  check: "M3 8.6 6.4 12 13 4.5",
  more: "M3.5 8h.01M8 8h.01M12.5 8h.01",
  expand: "M9.5 2.5h4v4M13.5 2.5 9 7M6.5 13.5h-4v-4M2.5 13.5 7 9",
  chevronDown: "M3.5 6 8 10.5 12.5 6",
  link: "M6.7 9.3a2.8 2.8 0 0 0 4 0l2-2a2.8 2.8 0 0 0-4-4l-.8.8M9.3 6.7a2.8 2.8 0 0 0-4 0l-2 2a2.8 2.8 0 0 0 4 4l.8-.8",
  ticket: "M2.5 4.5h11v2.2a1.3 1.3 0 0 0 0 2.6v2.2h-11V9.3a1.3 1.3 0 0 0 0-2.6zM9.5 4.5v7",
  edit: "M10.6 3.1a1.5 1.5 0 0 1 2.1 2.1L5.4 12.6l-2.9.8.8-2.9zM9.5 4.2l2.1 2.1",
  trash:
    "M2.5 4.5h11M6.2 4.5V3a.5.5 0 0 1 .5-.5h2.6a.5.5 0 0 1 .5.5v1.5M4 4.5l.6 8.1a1 1 0 0 0 1 .9h4.8a1 1 0 0 0 1-.9l.6-8.1",
  know: "M8 4.5C6.5 3.5 4.5 3.3 2.5 3.5v9c2-.2 4 0 5.5 1 1.5-1 3.5-1.2 5.5-1v-9c-2-.2-4 0-5.5 1zM8 4.5v9",
} as const;

export type NavIcon = keyof typeof NAV_PATHS;

export function Icon({ name, size = 15, weight = 1.5 }: { name: NavIcon; size?: number; weight?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={weight}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={NAV_PATHS[name]} />
    </svg>
  );
}

/** The same icons as components, for a slot that wants one (SideNavItem). */
export const NAV_ICONS = {} as Record<NavIcon, ComponentType>;
for (const name of Object.keys(NAV_PATHS) as NavIcon[]) NAV_ICONS[name] = () => <Icon name={name} />;

const GLYPHS = {
  warn: "M7.25 4.6h1.5v4.2h-1.5zM8 10.2a.95.95 0 1 1 0 1.9a.95.95 0 1 1 0-1.9z",
  ask: "M8 10.4a.85.85 0 1 1 0 1.7a.85.85 0 1 1 0-1.7zM6 6.4a2 2 0 1 1 2.9 1.8c-.4.2-.5.4-.5.8v.6H7.6V9c0-.8.4-1.3 1-1.6a1 1 0 1 0-1.4-1z",
  half: "M8 3.5a4.5 4.5 0 0 1 0 9z",
  dot: "M8 5.5a2.5 2.5 0 1 1 0 5a2.5 2.5 0 1 1 0-5z",
} as const;

export function StatusGlyph({ glyph, color }: { glyph: keyof typeof GLYPHS; color: string }) {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" {...stylex.props(styles.fixed)}>
      <circle cx="8" cy="8" r="6.5" fill="none" stroke={color} strokeWidth="1.5" />
      <path d={GLYPHS[glyph]} fill={color} />
    </svg>
  );
}

export function SearchIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <circle cx="7" cy="7" r="4.2" />
      <path d="m10.2 10.2 3.3 3.3" />
    </svg>
  );
}

export function NewIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8.5 3H4a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V7.5" />
      <path d="m11.5 2.5 2 2L8 10H6V8z" />
    </svg>
  );
}

/** The stone-cut "A" of the app icon (`scripts/make-icon.py`), on its green tile. */
export function Logo({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 120 120"
      role="img"
      aria-label="Aurelius"
      {...stylex.props(styles.fixed)}
    >
      <rect width="120" height="120" rx="28" fill="var(--color-accent)" />
      <path
        transform="translate(60 60) scale(.72) translate(-60 -60)"
        fill="#fff"
        d="M60 14 104 106H82L60 56 38 106H16Z"
      />
    </svg>
  );
}
