import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type InputHTMLAttributes, type ReactNode, useState } from "react";
import { useLinkState } from "../connection";
import { Button } from "../ui/kit";

/** A read of a settings section; refetched by events and after every change here. */
export function useSetting<T>(name: string, read: () => Promise<T>) {
  const { status, user } = useLinkState();
  return useQuery({
    queryKey: ["settings", name],
    queryFn: read,
    enabled: status === "online" && user !== null,
    staleTime: 10_000,
  });
}

/** One change at a time, its error shown beside it, and the section read again after it. */
export function useChange() {
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (change: () => Promise<unknown>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      await change();
      await client.invalidateQueries({ queryKey: ["settings"] });
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "This did not work.");
      return false;
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run };
}

export function Problem({ error }: { error: { message: string } | string | null | undefined }) {
  if (!error) return null;
  return (
    <p role="alert" className="m-0 text-red">
      {typeof error === "string" ? error : error.message}
    </p>
  );
}

/** A titled group. Without a title it is only the card. */
export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-[10px]">
      {title ? <h2 className="m-0 text-[12px] font-medium text-text3">{title}</h2> : null}
      <div className="flex flex-col rounded-[10px] border border-line [&>*:not(:last-child)]:border-b [&>*:not(:last-child)]:border-line2">
        {children}
      </div>
    </section>
  );
}

export function Row({ title, hint, children }: { title: ReactNode; hint?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex items-center gap-4 px-4 py-[14px]">
      <div className="min-w-0 flex-1">
        <div className="font-medium [overflow-wrap:anywhere]">{title}</div>
        {hint ? (
          <div className="mt-[2px] text-[12px] leading-[1.5] text-text3 [overflow-wrap:anywhere]">{hint}</div>
        ) : null}
      </div>
      {children ? <div className="flex flex-none flex-wrap items-center justify-end gap-2">{children}</div> : null}
    </div>
  );
}

export function Switch({
  label,
  on,
  disabled,
  onChange,
}: {
  label: string;
  on: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className={`relative h-[18px] w-8 flex-none cursor-pointer rounded-[9px] border-0 p-0 disabled:cursor-default disabled:opacity-50 ${on ? "bg-acc" : "bg-track"}`}
    >
      <span
        className={`absolute top-[2px] left-[2px] size-[14px] rounded-full bg-white transition-transform duration-[120ms] ${on ? "translate-x-[14px]" : ""}`}
      />
    </button>
  );
}

export const SmallButton: typeof Button = ({ className = "", ...props }) => (
  <Button className={`h-7 px-[10px] text-[12px] ${className}`} {...props} />
);

/** Hard to undo: asks in place, so a stray click does nothing. */
export function ConfirmButton({
  label,
  question,
  onConfirm,
  disabled,
}: {
  label: string;
  question: string;
  onConfirm: () => void;
  disabled?: boolean;
}) {
  const [asking, setAsking] = useState(false);
  if (!asking) {
    return (
      <SmallButton kind="danger" disabled={disabled ?? false} onClick={() => setAsking(true)}>
        {label}
      </SmallButton>
    );
  }
  return (
    <>
      <span className="text-[12px] text-text2">{question}</span>
      <SmallButton
        kind="danger"
        onClick={() => {
          setAsking(false);
          onConfirm();
        }}
      >
        Yes, {label.toLowerCase()}
      </SmallButton>
      <SmallButton onClick={() => setAsking(false)}>Cancel</SmallButton>
    </>
  );
}

export function TextInput({
  label,
  className = "",
  ...props
}: { label: string } & InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      aria-label={label}
      className={`h-7 min-w-0 rounded-[6px] border border-btn-line bg-sunk px-2 text-[12px] text-text ${className}`}
      {...props}
    />
  );
}

/** A text that is only sent when it changed. */
export function NameEditor({
  label,
  value,
  onSave,
  busy,
}: {
  label: string;
  value: string;
  onSave: (value: string) => void;
  busy: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? value;
  const changed = draft !== null && draft.trim() !== "" && draft.trim() !== value;
  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (!changed) return;
        onSave(shown.trim());
        setDraft(null);
      }}
    >
      <TextInput label={label} value={shown} onChange={(e) => setDraft(e.target.value)} className="w-[200px]" />
      {changed ? (
        <SmallButton kind="primary" type="submit" disabled={busy}>
          Save
        </SmallButton>
      ) : null}
    </form>
  );
}

export function Muted({ children }: { children: ReactNode }) {
  return <p className="m-0 text-[12px] leading-[1.5] text-text3">{children}</p>;
}

export function when(ms: number | null | undefined): string {
  return ms ? new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : "Never";
}

export function bytes(n: number): string {
  if (n < 1024 * 1024) return `${Math.max(1, Math.round(n / 1024))} KB`;
  if (n < 1024 * 1024 * 1024) return `${Math.round(n / (1024 * 1024))} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/** "claude-opus-5-5" → "Opus 5.5". */
export function modelName(id: string): string {
  const parts = id.replace(/^claude-/, "").split("-");
  const words = parts.filter((p) => !/^\d+$/.test(p)).map((w) => w.charAt(0).toUpperCase() + w.slice(1));
  const version = parts.filter((p) => /^\d+$/.test(p)).join(".");
  return [...words, version].filter(Boolean).join(" ");
}
