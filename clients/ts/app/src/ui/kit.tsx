import type { ButtonHTMLAttributes, ReactNode } from "react";

export function Kbd({ children, onAccent = false }: { children: ReactNode; onAccent?: boolean }) {
  return (
    <span className={`rounded-[4px] px-[5px] py-px text-[11px] ${onAccent ? "bg-white/18" : "bg-kbd text-text2"}`}>
      {children}
    </span>
  );
}

type ButtonKind = "primary" | "secondary" | "danger" | "ghost";

const KINDS: Record<ButtonKind, string> = {
  primary: "border-acc-line bg-acc text-white hover:brightness-110",
  secondary: "border-btn-line bg-btn text-text hover:bg-sel",
  danger: "border-btn-line bg-btn text-red hover:bg-sel",
  ghost: "border-transparent bg-transparent text-text2 hover:bg-sel hover:text-text",
};

export function Button({
  kind = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { kind?: ButtonKind }) {
  return (
    <button
      type="button"
      className={`inline-flex h-8 cursor-pointer items-center gap-2 rounded-[6px] border px-3 text-[13px] font-medium disabled:cursor-default disabled:opacity-50 ${KINDS[kind]} ${className}`}
      {...props}
    />
  );
}

/** A square with an initial, for a project or an employee. */
export function Mark({
  letter,
  color,
  size = 18,
  radius = 5,
}: {
  letter: string;
  color: string;
  size?: number;
  radius?: number;
}) {
  return (
    <span
      aria-hidden="true"
      className="grid flex-none place-items-center font-semibold text-white"
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        background: color,
        fontSize: Math.max(9, Math.round(size * 0.55)),
      }}
    >
      {letter}
    </span>
  );
}

const MARKS = ["#3E5BB8", "#1F7A4C", "#A0641E", "#8A4A9A", "#4A63C8", "#2F7A57", "#6B4FA3", "#55595F"];

/** The same id always gets the same color. */
export function markColor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i++) hash = (hash * 31 + id.charCodeAt(i)) | 0;
  return MARKS[Math.abs(hash) % MARKS.length] as string;
}

export function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

/** The design's little choices with a name in front: "Priority Normal". Opens a menu or cycles. */
export function Chip({
  name,
  children,
  ...props
}: { name?: string; children: ReactNode } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className="inline-flex h-[26px] cursor-pointer items-center gap-[6px] rounded-[6px] border border-line bg-transparent px-[9px] text-[12px] text-text2 hover:bg-sel hover:text-text"
      {...props}
    >
      {name ? <span className="text-text3">{name}</span> : null}
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
    <fieldset className="m-0 flex gap-1 border-0 p-0" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.key}
          type="button"
          aria-pressed={o.key === value}
          onClick={() => onChange(o.key)}
          className={`h-6 cursor-pointer rounded-[5px] border px-[9px] text-[12px] font-medium ${o.key === value ? "border-btn-line bg-sel text-text" : "border-transparent bg-transparent text-text2 hover:text-text"}`}
        >
          {o.label}
        </button>
      ))}
    </fieldset>
  );
}
