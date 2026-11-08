// The design's name-and-value control, which Astryx has no counterpart for.

import * as stylex from "@stylexjs/stylex";
import type { ButtonHTMLAttributes, ReactNode } from "react";

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
