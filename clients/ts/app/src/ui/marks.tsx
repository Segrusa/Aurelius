// A square with an initial, for a project or an employee: Astryx's Avatar has no "→", no sizes between
// its steps and no color per id.

import * as stylex from "@stylexjs/stylex";

const styles = stylex.create({
  mark: {
    display: "grid",
    flexShrink: 0,
    placeItems: "center",
    color: "#fff",
    fontWeight: 600,
  },
});

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
      {...stylex.props(styles.mark)}
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
