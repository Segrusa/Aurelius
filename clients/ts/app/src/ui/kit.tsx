// Adapters from the old kit to Astryx, for screens not migrated yet. Goes when nothing imports it.

import { Button as AstryxButton } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import type { ButtonHTMLAttributes, ReactNode } from "react";

export { Chip } from "./chip";
export { KeyHint as Kbd } from "./key-hint";
export { initial, Mark, markColor } from "./marks";

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
