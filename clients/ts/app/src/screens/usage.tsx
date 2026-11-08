import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Card } from "@astryxdesign/core/Card";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { expectBody } from "@aurelius/client";
import type { UsageHours, UsageRow, UsageUserRow, UsageWindow } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed, useSubscription, useUsage } from "../data";
import { Eyebrow, failureOf, Loading, Page, PageHead, shortDay } from "./org-kit";
import { bars, clampPercent, costText, limitTone, subscriptionFor, tokensShort, userRowName } from "./usage-logic";

const FETCH_AGAIN_MS = 6000;
const MONO = "var(--font-family-code)";
const TEXT3 = "var(--precision-text3)";
const TEXT2 = "var(--color-text-secondary)";

const styles = stylex.create({
  card: { borderRadius: 10, backgroundColor: "var(--color-background-surface)" },
  cardBody: { display: "flex", flexDirection: "column" },
  gap2: { gap: 8 },
  gap3: { gap: 12 },
  gap4: { gap: 16 },
  gap6: { gap: 6 },
  pad16: { padding: 16 },
  pad18: { padding: 18 },
  line: { display: "flex", alignItems: "center", gap: 8 },
  end: { marginInlineStart: "auto", display: "flex", gap: 4 },
  title: { margin: 0, fontSize: 14, fontWeight: 600 },
  quiet: { margin: 0, color: TEXT3 },
  second: { margin: 0, color: TEXT2 },
  raw: { margin: 0, fontFamily: MONO, fontSize: 12, whiteSpace: "pre-wrap", color: TEXT2 },
  limit: { display: "flex", flexDirection: "column", gap: 8 },
  between: { display: "flex", justifyContent: "space-between" },
  strong: { fontWeight: 500 },
  mono: { fontFamily: MONO },
  track: { height: 6, borderRadius: 3, backgroundColor: "var(--color-track)" },
  fill: { height: 6, borderRadius: 3 },
  small: { margin: 0, fontSize: 12, color: TEXT3 },
  smallMono: { margin: 0, fontFamily: MONO, fontSize: 12, color: TEXT2 },
  tileLabel: { fontSize: 12, color: TEXT3 },
  tileValue: { fontSize: 26, lineHeight: 1.15, fontWeight: 600, letterSpacing: "-0.02em" },
  hours: {
    display: "flex",
    height: 100,
    alignItems: "flex-end",
    gap: 3,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
  },
  hour: { display: "flex", minWidth: 0, flex: 1, flexDirection: "column", justifyContent: "flex-end" },
  output: { backgroundColor: "var(--color-text-accent)" },
  input: { borderStartStartRadius: 1, borderStartEndRadius: 1, backgroundColor: "var(--precision-bar)" },
  legend: { display: "flex", gap: 16, fontSize: 12, color: TEXT3 },
  legendItem: { display: "flex", alignItems: "center", gap: 6 },
  swatch: { width: 8, height: 8, borderRadius: 2 },
  swatchInput: { backgroundColor: "var(--precision-bar)" },
  swatchOutput: { backgroundColor: "var(--color-text-accent)" },
  tableScroll: { overflowX: "auto" },
  table: { minWidth: 640, fontSize: 12 },
  headCell: { fontSize: 12, fontWeight: 400, color: TEXT3 },
  cell: { color: TEXT2 },
  cellMono: { fontFamily: MONO, color: TEXT2 },
  truncate: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  scroller: { minHeight: 0, flex: 1, overflowY: "auto" },
  page: { display: "flex", maxWidth: 1040, flexDirection: "column", gap: 24, padding: 24 },
  section: { display: "flex", flexDirection: "column", gap: 12 },
  eyebrow: { margin: 0, fontSize: 12, fontWeight: 500, color: TEXT3 },
  tiles: { display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: 12 },
  days: {
    display: "flex",
    height: 180,
    alignItems: "flex-end",
    gap: 8,
    borderBlockEndWidth: 1,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
  },
  day: {
    display: "flex",
    height: "100%",
    minWidth: 0,
    flex: 1,
    cursor: "pointer",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: 4,
    borderWidth: 0,
    borderStartStartRadius: 3,
    borderStartEndRadius: 3,
    padding: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  dayOn: { backgroundColor: "var(--color-overlay-hover)" },
  dayValue: { fontFamily: MONO, fontSize: 11, color: TEXT2 },
  dayBar: { width: "100%", borderStartStartRadius: 3, borderStartEndRadius: 3 },
  barToday: { backgroundColor: "var(--color-text-accent)" },
  barEmpty: { backgroundColor: "var(--color-track)" },
  barBusy: { backgroundColor: "var(--precision-bar)" },
  dates: { display: "flex", gap: 8 },
  date: { minWidth: 0, flex: 1, textAlign: "center", fontSize: 11, color: TEXT3 },
  note: { margin: 0, fontSize: 12, lineHeight: 1.6, color: TEXT3 },
});

/** The design's panel: Astryx's Card, painted as the surface it sits on. */
function Panel({ xstyle, children }: { xstyle: stylex.StyleXStyles; children: ReactNode }) {
  return (
    <Card padding={0} xstyle={styles.card}>
      <div {...stylex.props(styles.cardBody, xstyle)}>{children}</div>
    </Card>
  );
}

function Subscription() {
  const connection = useConnection();
  const client = useQueryClient();
  const { user } = useLinkState();
  const query = useSubscription();
  const [busy, setBusy] = useState(false);
  const data = subscriptionFor(query.data, user?.id ?? null);
  const refresh = async () => {
    setBusy(true);
    try {
      const body = await connection.call("subscription.get", { refresh: true });
      client.setQueryData(["subscription"], expectBody(body, "subscription").value);
    } catch {
      // The card keeps what it has; the next read tries again.
    } finally {
      setBusy(false);
    }
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: fresh once, on opening the page.
  useEffect(() => {
    void refresh();
  }, []);
  // The server fetches in the background and has nothing yet: look again shortly.
  useEffect(() => {
    if (data) return;
    const again = setTimeout(() => void client.invalidateQueries({ queryKey: ["subscription"] }), FETCH_AGAIN_MS);
    return () => clearTimeout(again);
  }, [data, client]);
  return (
    <Panel xstyle={[styles.gap4, styles.pad18]}>
      <div {...stylex.props(styles.line)}>
        <Heading level={2} xstyle={styles.title}>
          Claude subscription
        </Heading>
        {data?.plan ? <Badge label={data.plan.charAt(0).toUpperCase() + data.plan.slice(1)} /> : null}
        <span {...stylex.props(styles.end)}>
          <Button size="sm" label="Refresh" isDisabled={busy} onClick={() => void refresh()} />
        </span>
      </div>
      {!data ? <p {...stylex.props(styles.quiet)}>Fetching via Claude Code…</p> : null}
      {data?.error ? <p {...stylex.props(styles.second)}>{data.error}</p> : null}
      {data && !data.error && data.limits.length === 0 && data.raw ? (
        <pre {...stylex.props(styles.raw)}>{data.raw.split("\n").slice(0, 8).join("\n")}</pre>
      ) : null}
      {data?.limits.map((l) => {
        const percent = clampPercent(l.percent);
        return (
          <div key={l.label} {...stylex.props(styles.limit)}>
            <div {...stylex.props(styles.between)}>
              <span {...stylex.props(styles.strong)}>{l.label}</span>
              <span {...stylex.props(styles.mono)}>{percent}%</span>
            </div>
            <div {...stylex.props(styles.track)}>
              <div
                {...stylex.props(styles.fill)}
                style={{ width: `${percent}%`, backgroundColor: limitTone(percent) }}
              />
            </div>
            {l.resets ? <span {...stylex.props(styles.small)}>Resets {l.resets}</span> : null}
          </div>
        );
      })}
      {data && data.limits.length > 0 ? (
        <p {...stylex.props(styles.small)}>
          Via Claude Code (/usage) with your own login; free to check. Includes usage outside Aurelius.
        </p>
      ) : null}
    </Panel>
  );
}

function Tile({ w }: { w: UsageWindow }) {
  return (
    <Panel xstyle={[styles.gap6, styles.pad16]}>
      <span {...stylex.props(styles.tileLabel)}>{w.label}</span>
      <span {...stylex.props(styles.tileValue)}>{tokensShort(w.input_tokens + w.output_tokens)}</span>
      <span {...stylex.props(styles.smallMono)}>
        in {tokensShort(w.input_tokens)} · out {tokensShort(w.output_tokens)} · cache {tokensShort(w.cached_tokens)}
      </span>
      <span {...stylex.props(styles.tileLabel)}>
        {w.runs} runs · {costText(w.cost_micro_usd, w.estimate)}
      </span>
    </Panel>
  );
}

function Hours({ day, onMove }: { day: number; onMove: (delta: number) => void }) {
  const connection = useConnection();
  const hours = useKeyed<UsageHours>(
    ["usage", "hours", day],
    async () => expectBody(await connection.call("usage.get_hours", { day_start_ms: day }), "usage_hours").value,
  );
  const list = hours.data?.hours ?? [];
  const sum = (pick: (h: UsageHours["hours"][number]) => number) => list.reduce((n, h) => n + pick(h), 0);
  const input = sum((h) => h.input_tokens);
  const output = sum((h) => h.output_tokens);
  const max = Math.max(1, ...list.map((h) => h.input_tokens + h.output_tokens));
  return (
    <Panel xstyle={[styles.gap3, styles.pad18]}>
      <div {...stylex.props(styles.line)}>
        <Eyebrow>PER HOUR · {shortDay(day).toUpperCase()}</Eyebrow>
        <span {...stylex.props(styles.end)}>
          <Button size="sm" label="Previous day" onClick={() => onMove(-1)} />
          <Button size="sm" label="Next day" onClick={() => onMove(1)} />
        </span>
      </div>
      {hours.isPending ? <Loading rows={2} /> : null}
      {hours.error ? <Banner status="error" role="alert" title={failureOf(hours.error)} /> : null}
      {hours.data ? (
        <>
          <p {...stylex.props(styles.smallMono)}>
            {tokensShort(input + output)} tokens · in {tokensShort(input)} · out {tokensShort(output)} · cache{" "}
            {tokensShort(sum((h) => h.cached_tokens))} · {sum((h) => h.runs)} runs
          </p>
          <div role="img" aria-label="Tokens per hour" {...stylex.props(styles.hours)}>
            {list.map((h) => {
              const at = new Date(h.hour_start_ms).getHours();
              return (
                <div
                  key={h.hour_start_ms}
                  title={`${String(at).padStart(2, "0")}:00 · ${tokensShort(h.input_tokens + h.output_tokens)} tokens`}
                  {...stylex.props(styles.hour)}
                >
                  <div {...stylex.props(styles.output)} style={{ height: `${(h.output_tokens / max) * 96}px` }} />
                  <div {...stylex.props(styles.input)} style={{ height: `${(h.input_tokens / max) * 96}px` }} />
                </div>
              );
            })}
          </div>
          <div {...stylex.props(styles.legend)}>
            <span {...stylex.props(styles.legendItem)}>
              <span {...stylex.props(styles.swatch, styles.swatchInput)} />
              Input
            </span>
            <span {...stylex.props(styles.legendItem)}>
              <span {...stylex.props(styles.swatch, styles.swatchOutput)} />
              Output
            </span>
          </div>
        </>
      ) : null}
    </Panel>
  );
}

const COLUMNS = ["Connection", "Model", "Runs", "Input", "Output", "Cost"];
const USER_COLUMNS = ["Board member", "Runs", "Calls", "Input", "Output", "Cost"];

function People({ rows }: { rows: UsageUserRow[] }) {
  if (rows.length === 0) return null;
  return (
    <Panel xstyle={[styles.gap2, styles.pad18]}>
      <Eyebrow>PER BOARD MEMBER · 30 DAYS</Eyebrow>
      <div {...stylex.props(styles.tableScroll)}>
        <Table density="compact" xstyle={styles.table}>
          <TableHeader>
            <TableRow isHeaderRow>
              {USER_COLUMNS.map((c) => (
                <TableHeaderCell key={c} xstyle={styles.headCell}>
                  {c}
                </TableHeaderCell>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((r) => (
              <TableRow key={r.login_user ?? "none"}>
                <TableCell xstyle={styles.truncate}>{userRowName(r)}</TableCell>
                <TableCell xstyle={styles.cell}>{r.runs}</TableCell>
                <TableCell xstyle={styles.cell}>{r.model_calls || "–"}</TableCell>
                <TableCell xstyle={styles.cellMono}>{tokensShort(r.input_tokens)}</TableCell>
                <TableCell xstyle={styles.cellMono}>{tokensShort(r.output_tokens)}</TableCell>
                <TableCell xstyle={styles.cell}>{costText(r.cost_micro_usd, r.estimate)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </Panel>
  );
}

function Sources({ rows }: { rows: UsageRow[] }) {
  return (
    <Panel xstyle={[styles.gap2, styles.pad18]}>
      <Eyebrow>PER CONNECTION AND MODEL · 30 DAYS</Eyebrow>
      {rows.length === 0 ? <EmptyState title="No usage yet." headingLevel={2} isCompact /> : null}
      {rows.length > 0 ? (
        <div {...stylex.props(styles.tableScroll)}>
          <Table density="compact" xstyle={styles.table}>
            <TableHeader>
              <TableRow isHeaderRow>
                {COLUMNS.map((c) => (
                  <TableHeaderCell key={c} xstyle={styles.headCell}>
                    {c}
                  </TableHeaderCell>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((r) => (
                <TableRow key={`${r.connection}|${r.model}`}>
                  <TableCell xstyle={styles.truncate}>{r.connection}</TableCell>
                  <TableCell xstyle={[styles.truncate, styles.cellMono]}>{r.model}</TableCell>
                  <TableCell xstyle={styles.cell}>{r.runs}</TableCell>
                  <TableCell xstyle={styles.cellMono}>{tokensShort(r.input_tokens)}</TableCell>
                  <TableCell xstyle={styles.cellMono}>{tokensShort(r.output_tokens)}</TableCell>
                  <TableCell xstyle={styles.cell}>{costText(r.cost_micro_usd, r.estimate)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      ) : null}
    </Panel>
  );
}

export function UsageScreen() {
  const usage = useUsage();
  const [day, setDay] = useState<number | null>(null);
  const days = usage.data?.by_day ?? [];
  const drawn = useMemo(() => bars(days, Date.now()), [days]);
  const shown = day ?? drawn.find((d) => d.today)?.day_start_ms ?? drawn.at(-1)?.day_start_ms ?? null;
  const move = (delta: number) => {
    const at = drawn.findIndex((d) => d.day_start_ms === shown);
    const next = drawn[at + delta];
    if (next) setDay(next.day_start_ms);
  };
  return (
    <Page>
      <PageHead title="Usage" />
      <div {...stylex.props(styles.scroller)}>
        <div {...stylex.props(styles.page)}>
          <Subscription />
          {usage.isPending ? <Loading /> : null}
          {usage.error ? (
            <Banner status="error" role="alert" title={`The usage did not load: ${failureOf(usage.error)}`} />
          ) : null}
          {usage.data ? (
            <>
              <section {...stylex.props(styles.section)}>
                <p {...stylex.props(styles.eyebrow)}>Aurelius · tokens</p>
                <div {...stylex.props(styles.tiles)}>
                  {usage.data.windows.map((w) => (
                    <Tile key={w.label} w={w} />
                  ))}
                </div>
              </section>
              <Panel xstyle={[styles.gap3, styles.pad18]}>
                <p {...stylex.props(styles.eyebrow)}>Tokens per day · last 14 days</p>
                <div {...stylex.props(styles.days)}>
                  {drawn.map((d) => (
                    <button
                      key={d.day_start_ms}
                      type="button"
                      aria-pressed={d.day_start_ms === shown}
                      aria-label={`${shortDay(d.day_start_ms)} · ${d.tokens} tokens`}
                      title={`${shortDay(d.day_start_ms)} · ${tokensShort(d.tokens)} tokens`}
                      onClick={() => setDay(d.day_start_ms)}
                      {...stylex.props(styles.day, d.day_start_ms === shown && styles.dayOn)}
                    >
                      <span {...stylex.props(styles.dayValue)}>{d.empty ? "" : tokensShort(d.tokens)}</span>
                      <span
                        {...stylex.props(
                          styles.dayBar,
                          d.today ? styles.barToday : d.empty ? styles.barEmpty : styles.barBusy,
                        )}
                        style={{ height: d.height }}
                      />
                    </button>
                  ))}
                </div>
                <div {...stylex.props(styles.dates)}>
                  {drawn.map((d) => (
                    <span key={d.day_start_ms} {...stylex.props(styles.date)}>
                      {new Date(d.day_start_ms).getDate()}
                    </span>
                  ))}
                </div>
              </Panel>
              {shown !== null ? <Hours day={shown} onMove={move} /> : null}
              <Sources rows={usage.data.by_source} />
              <People rows={usage.data.by_user ?? []} />
              <p {...stylex.props(styles.note)}>
                Claude Code on your subscription: costs are the API-equivalent value for reference; you pay nothing
                extra. API keys: tokens as reported by the provider. The local test runtime estimates tokens (4
                characters ≈ 1 token).
              </p>
            </>
          ) : null}
        </div>
      </div>
    </Page>
  );
}
