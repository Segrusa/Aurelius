// Numbers of the Usage screen, as the desktop app writes them (ui.rs `fmt_tokens`, `fmt_cost`).

export function tokensShort(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(2)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n < 1000) return String(n);
  return `${(n / 1000).toFixed(1).replace(/\.0$/, "")}k`;
}

export function costText(microUsd: number | null | undefined, estimate: boolean): string {
  if (microUsd === null || microUsd === undefined) return "cost not reported";
  if (microUsd === 0) return "no cost";
  const usd = microUsd / 1_000_000;
  return `$${usd.toFixed(usd < 0.01 ? 4 : 2)}${estimate ? " (estimate)" : ""}`;
}

/** The colour of a limit's bar: amber from 70%, red from 90%. */
export const limitTone = (percent: number) =>
  percent >= 90 ? "var(--color-error)" : percent >= 70 ? "var(--color-warning)" : "var(--color-text-accent)";

export const clampPercent = (percent: number) => Math.max(0, Math.min(100, Math.round(percent)));

/** Bar height in the 180 px plot: room for the label on top, and a sliver for anything above zero. */
export const barHeight = (tokens: number, max: number) => (max > 0 ? Math.max(2, (tokens / max) * 140) : 2);

export function bars(days: { day_start_ms: number; tokens: number }[], todayMs: number) {
  const max = Math.max(0, ...days.map((d) => d.tokens));
  const today = new Date(todayMs).toDateString();
  return days.map((d) => ({
    ...d,
    height: barHeight(d.tokens, max),
    today: new Date(d.day_start_ms).toDateString() === today,
    empty: d.tokens === 0,
  }));
}

/** Whose share a row of the usage per board member is. */
export const userRowName = (row: { login_user?: string | null; username?: string | null }): string =>
  row.username ?? (row.login_user ? "Former board member" : "No Claude login");

/** The answer of `subscription.get`, only when it is of this member's own login (`own_subscription` in the desktop app). */
export const subscriptionFor = <T extends { login_user?: string | null; fetched_at_ms: number }>(
  data: T | undefined,
  me: string | null,
): T | null => {
  if (!data || data.fetched_at_ms === 0 || me === null) return null;
  return data.login_user === me ? data : null;
};
