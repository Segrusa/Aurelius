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
