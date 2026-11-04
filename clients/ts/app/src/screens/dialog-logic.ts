// The rules of New ticket and Give work, pure. Port of parse_deadline in clients/desktop/src/screens/board.rs.

const WRONG = "Write the date as 2026-11-03, optionally with a time: 2026-11-03 17:00.";

/** Milliseconds, or null for an empty field. A date alone means the end of that day. Throws `WRONG`. */
export function parseDeadline(text: string, offsetMinutes: number): number | null {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const [date = "", time, ...more] = trimmed.split(/[ T]+/);
  const parts = date.split("-").map(Number);
  const [y, m, d] = parts;
  if (
    more.length > 0 ||
    parts.length !== 3 ||
    y === undefined ||
    m === undefined ||
    d === undefined ||
    !parts.every(Number.isInteger)
  )
    throw new Error(WRONG);
  if (y < 1970 || y > 9999 || m < 1 || m > 12 || d < 1 || d > 31) throw new Error(WRONG);
  const day = new Date(Date.UTC(y, m - 1, d));
  if (day.getUTCMonth() !== m - 1) throw new Error(WRONG);
  let minutes = 23 * 60 + 59;
  if (time !== undefined) {
    const match = /^(\d{1,2}):(\d{2})$/.exec(time);
    const [hh, mm] = [Number(match?.[1]), Number(match?.[2])];
    if (!match || hh > 23 || mm > 59) throw new Error(WRONG);
    minutes = hh * 60 + mm;
  }
  return day.getTime() + (minutes - offsetMinutes) * 60_000;
}

export function parseLabels(text: string): string[] {
  return [
    ...new Set(
      text
        .split(",")
        .map((l) => l.trim())
        .filter(Boolean),
    ),
  ];
}

const TITLE_CHARS = 60;

/** A title from what needs doing: the first line, shortened. Port of `derive_title` in the domain crate. */
export function titleOfGoal(goal: string): string {
  const first = [...(goal.split("\n", 1)[0]?.trim() ?? "")];
  return first.slice(0, TITLE_CHARS).join("") + (first.length > TITLE_CHARS ? "…" : "");
}

interface Assignable {
  id: string;
  status: string;
  reports_to?: string | null;
}

/** Active employees without an active boss: the top of the org chart. */
export function rootsOf<T extends Assignable>(employees: T[]): T[] {
  const present = (id: string) => employees.some((e) => e.id === id && e.status !== "archived");
  return employees.filter((e) => e.status === "active" && !(e.reports_to && present(e.reports_to)));
}

/** Whom work goes to unless picked: the only root; of several, the last one given work, else the first. */
export function defaultAssignee<T extends Assignable>(employees: T[], last: string | null): string | null {
  const roots = rootsOf(employees);
  if (roots.length <= 1) return roots[0]?.id ?? null;
  const known = employees.some((e) => e.id === last && e.status === "active");
  return known ? last : (roots[0]?.id ?? null);
}

/** The next of a list, around; for the keys that cycle a choice. */
export function cycle<T>(options: readonly T[], current: T): T {
  const at = options.indexOf(current);
  return options[(at + 1) % options.length] as T;
}
