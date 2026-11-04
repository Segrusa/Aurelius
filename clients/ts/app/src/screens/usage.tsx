import { expectBody } from "@aurelius/client";
import type { UsageHours, UsageRow, UsageWindow } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed, useSubscription, useUsage } from "../data";
import { Button } from "../ui/kit";
import { EYEBROW, failureOf, PageHead, SMALL, shortDay } from "./org-kit";
import { bars, clampPercent, costText, limitTone, subscriptionFor, tokensShort } from "./usage-logic";

const FETCH_AGAIN_MS = 6000;
const CARD = "flex flex-col rounded-[10px] border border-line bg-panel";

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
    <section className={`${CARD} gap-4 p-[18px]`}>
      <div className="flex items-center gap-2">
        <h2 className="m-0 text-[14px] font-semibold">Claude subscription</h2>
        {data?.plan ? (
          <span className="rounded-[4px] border border-line px-[7px] py-px text-[12px] text-text2 capitalize">
            {data.plan}
          </span>
        ) : null}
        <Button className={`${SMALL} ml-auto`} disabled={busy} onClick={() => void refresh()}>
          Refresh
        </Button>
      </div>
      {!data ? <p className="m-0 text-text3">Fetching via Claude Code…</p> : null}
      {data?.error ? <p className="m-0 text-text2">{data.error}</p> : null}
      {data && !data.error && data.limits.length === 0 && data.raw ? (
        <pre className="m-0 font-mono text-[12px] whitespace-pre-wrap text-text2">
          {data.raw.split("\n").slice(0, 8).join("\n")}
        </pre>
      ) : null}
      {data?.limits.map((l) => {
        const percent = clampPercent(l.percent);
        return (
          <div key={l.label} className="flex flex-col gap-2">
            <div className="flex justify-between">
              <span className="font-medium">{l.label}</span>
              <span className="font-mono">{percent}%</span>
            </div>
            <div className="h-[6px] rounded-[3px] bg-track">
              <div className={`h-[6px] rounded-[3px] ${limitTone(percent)}`} style={{ width: `${percent}%` }} />
            </div>
            {l.resets ? <span className="text-[12px] text-text3">Resets {l.resets}</span> : null}
          </div>
        );
      })}
      {data && data.limits.length > 0 ? (
        <p className="m-0 text-[12px] text-text3">
          Via Claude Code (/usage) with your own login; free to check. Includes usage outside Aurelius.
        </p>
      ) : null}
    </section>
  );
}

function Tile({ w }: { w: UsageWindow }) {
  return (
    <div className={`${CARD} gap-[6px] p-4`}>
      <span className="text-[12px] text-text3">{w.label}</span>
      <span className="text-[26px] leading-[1.15] font-semibold tracking-[-0.02em]">
        {tokensShort(w.input_tokens + w.output_tokens)}
      </span>
      <span className="font-mono text-[12px] text-text2">
        in {tokensShort(w.input_tokens)} · out {tokensShort(w.output_tokens)} · cache {tokensShort(w.cached_tokens)}
      </span>
      <span className="text-[12px] text-text3">
        {w.runs} runs · {costText(w.cost_micro_usd, w.estimate)}
      </span>
    </div>
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
    <section className={`${CARD} gap-3 p-[18px]`}>
      <div className="flex items-center gap-2">
        <p className={EYEBROW}>PER HOUR · {shortDay(day).toUpperCase()}</p>
        <span className="ml-auto flex gap-1">
          <Button className={SMALL} onClick={() => onMove(-1)}>
            Previous day
          </Button>
          <Button className={SMALL} onClick={() => onMove(1)}>
            Next day
          </Button>
        </span>
      </div>
      {hours.isPending ? <p className="m-0 text-text3">Loading…</p> : null}
      {hours.error ? (
        <p role="alert" className="m-0 text-red">
          {failureOf(hours.error)}
        </p>
      ) : null}
      {hours.data ? (
        <>
          <p className="m-0 font-mono text-[12px] text-text2">
            {tokensShort(input + output)} tokens · in {tokensShort(input)} · out {tokensShort(output)} · cache{" "}
            {tokensShort(sum((h) => h.cached_tokens))} · {sum((h) => h.runs)} runs
          </p>
          <div
            role="img"
            aria-label="Tokens per hour"
            className="flex h-[100px] items-end gap-[3px] border-b border-line2"
          >
            {list.map((h) => {
              const at = new Date(h.hour_start_ms).getHours();
              return (
                <div
                  key={h.hour_start_ms}
                  title={`${String(at).padStart(2, "0")}:00 · ${tokensShort(h.input_tokens + h.output_tokens)} tokens`}
                  className="flex min-w-0 flex-1 flex-col justify-end"
                >
                  <div className="bg-acc-text" style={{ height: `${(h.output_tokens / max) * 96}px` }} />
                  <div className="rounded-t-[1px] bg-(--bar)" style={{ height: `${(h.input_tokens / max) * 96}px` }} />
                </div>
              );
            })}
          </div>
          <div className="flex gap-4 text-[12px] text-text3">
            <span className="flex items-center gap-[6px]">
              <span className="size-2 rounded-[2px] bg-(--bar)" />
              Input
            </span>
            <span className="flex items-center gap-[6px]">
              <span className="size-2 rounded-[2px] bg-acc-text" />
              Output
            </span>
          </div>
        </>
      ) : null}
    </section>
  );
}

function Sources({ rows }: { rows: UsageRow[] }) {
  const grid = "grid grid-cols-[minmax(140px,1fr)_minmax(140px,1fr)_60px_90px_90px_130px] items-center gap-3";
  return (
    <section className={`${CARD} gap-2 p-[18px]`}>
      <p className={EYEBROW}>PER CONNECTION AND MODEL · 30 DAYS</p>
      {rows.length === 0 ? <p className="m-0 text-text3">No usage yet.</p> : null}
      {rows.length > 0 ? (
        <div className="overflow-x-auto">
          <div className="min-w-[640px]">
            <div className={`${grid} h-7 border-b border-line2 text-[12px] text-text3`}>
              <span>Connection</span>
              <span>Model</span>
              <span>Runs</span>
              <span>Input</span>
              <span>Output</span>
              <span>Cost</span>
            </div>
            {rows.map((r) => (
              <div
                key={`${r.connection}|${r.model}`}
                className={`${grid} min-h-8 border-b border-line2 text-[12px] last:border-b-0`}
              >
                <span className="truncate">{r.connection}</span>
                <span className="truncate font-mono text-text2">{r.model}</span>
                <span className="text-text2">{r.runs}</span>
                <span className="font-mono text-text2">{tokensShort(r.input_tokens)}</span>
                <span className="font-mono text-text2">{tokensShort(r.output_tokens)}</span>
                <span className="text-text2">{costText(r.cost_micro_usd, r.estimate)}</span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </section>
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
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead title="Usage" />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex max-w-[1040px] flex-col gap-6 p-6">
          <Subscription />
          {usage.isPending ? <p className="m-0 text-text3">Loading usage…</p> : null}
          {usage.error ? (
            <p role="alert" className="m-0 text-red">
              The usage did not load: {failureOf(usage.error)}
            </p>
          ) : null}
          {usage.data ? (
            <>
              <section className="flex flex-col gap-3">
                <p className="m-0 text-[12px] font-medium text-text3">Aurelius · tokens</p>
                <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
                  {usage.data.windows.map((w) => (
                    <Tile key={w.label} w={w} />
                  ))}
                </div>
              </section>
              <section className={`${CARD} gap-3 p-[18px]`}>
                <p className="m-0 text-[12px] font-medium text-text3">Tokens per day · last 14 days</p>
                <div className="flex h-[180px] items-end gap-2 border-b border-line2">
                  {drawn.map((d) => (
                    <button
                      key={d.day_start_ms}
                      type="button"
                      aria-pressed={d.day_start_ms === shown}
                      aria-label={`${shortDay(d.day_start_ms)} · ${d.tokens} tokens`}
                      title={`${shortDay(d.day_start_ms)} · ${tokensShort(d.tokens)} tokens`}
                      onClick={() => setDay(d.day_start_ms)}
                      className={`flex h-full min-w-0 flex-1 cursor-pointer flex-col items-center justify-end gap-1 rounded-t-[3px] border-0 bg-transparent p-0 ${d.day_start_ms === shown ? "bg-sel" : "hover:bg-sel"}`}
                    >
                      <span className="font-mono text-[11px] text-text2">{d.empty ? "" : tokensShort(d.tokens)}</span>
                      <span
                        className={`w-full rounded-t-[3px] ${d.today ? "bg-acc-text" : d.empty ? "bg-track" : "bg-(--bar)"}`}
                        style={{ height: d.height }}
                      />
                    </button>
                  ))}
                </div>
                <div className="flex gap-2">
                  {drawn.map((d) => (
                    <span key={d.day_start_ms} className="min-w-0 flex-1 text-center text-[11px] text-text3">
                      {new Date(d.day_start_ms).getDate()}
                    </span>
                  ))}
                </div>
              </section>
              {shown !== null ? <Hours day={shown} onMove={move} /> : null}
              <Sources rows={usage.data.by_source} />
              <p className="m-0 text-[12px] leading-[1.6] text-text3">
                Claude Code on your subscription: costs are the API-equivalent value for reference; you pay nothing
                extra. API keys: tokens as reported by the provider. The local test runtime estimates tokens (4
                characters ≈ 1 token).
              </p>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}
