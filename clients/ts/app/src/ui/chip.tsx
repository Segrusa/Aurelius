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
  // Inside a composer the chips sit on its surface like text, as in a chat app's input.
  quiet: { borderColor: "transparent" },
  name: { color: "var(--precision-text3)" },
});

/** The design's little choices with a name in front: "Priority Normal". Opens a menu or cycles. */
export function Chip({
  name,
  quiet = false,
  children,
  className: _ignored,
  ...props
}: { name?: string; quiet?: boolean; children: ReactNode } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" {...stylex.props(styles.chip, quiet && styles.quiet)} {...props}>
      {name ? <span {...stylex.props(styles.name)}>{name}</span> : null}
      {children}
    </button>
  );
}
