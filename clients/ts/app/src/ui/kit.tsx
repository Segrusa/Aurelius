// The app's small controls on Astryx. Screens import Astryx directly; this stays for what has no
// direct counterpart: a Button that takes children, and the name-and-value Chip.

import { Button as AstryxButton } from "@astryxdesign/core/Button";
import { Kbd as AstryxKbd } from "@astryxdesign/core/Kbd";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import * as stylex from "@stylexjs/stylex";
import type { ButtonHTMLAttributes, ReactNode } from "react";

export { initial, Mark, markColor } from "./marks";

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

export function Kbd({ children, onAccent = false }: { children: string; onAccent?: boolean }) {
  return <AstryxKbd keys={keysOf(children)} {...(onAccent ? stylex.props(kbdStyles.onAccent) : {})} />;
}

type ButtonKind = "primary" | "secondary" | "danger" | "ghost";

const VARIANTS = { primary: "primary", secondary: "secondary", danger: "destructive", ghost: "ghost" } as const;

function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("").trim();
  return "";
}

export function Button({
  kind = "secondary",
  className,
  children,
  disabled,
  title,
  type,
  "aria-label": ariaLabel,
  onClick,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { kind?: ButtonKind; "data-autofocus"?: boolean | string }) {
  const label = ariaLabel ?? (textOf(children) || title || "Button");
  return (
    <AstryxButton
      {...(rest as object)}
      variant={VARIANTS[kind]}
      label={label}
      type={type === "submit" ? "submit" : "button"}
      isDisabled={Boolean(disabled)}
      {...(title ? { tooltip: title } : {})}
      {...(className ? { className } : {})}
      {...(onClick ? { onClick: onClick as never } : {})}
    >
      {typeof children === "string" ? undefined : children}
    </AstryxButton>
  );
}

const styles = stylex.create({
  chip: {
    display: "inline-flex",
    height: 26,
    cursor: "pointer",
    alignItems: "center",
    gap: 6,
    borderRadius: "var(--radius-element)",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 9,
    fontSize: 12,
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  name: { color: "var(--precision-text3)" },
});

/** The design's little choices with a name in front: "Priority Normal". Opens a menu or cycles. */
export function Chip({
  name,
  children,
  className: _ignored,
  ...props
}: { name?: string; children: ReactNode } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" {...stylex.props(styles.chip)} {...props}>
      {name ? <span {...stylex.props(styles.name)}>{name}</span> : null}
      {children}
    </button>
  );
}

/** A row of mutually exclusive choices. */
export function Segments<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { key: T; label: string }[];
  onChange: (key: T) => void;
}) {
  return (
    <SegmentedControl label={label} value={value} size="sm" onChange={(next) => onChange(next as T)}>
      {options.map((o) => (
        <SegmentedControlItem key={o.key} value={o.key} label={o.label} />
      ))}
    </SegmentedControl>
  );
}
