import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { Switch as AstryxSwitch } from "@astryxdesign/core/Switch";
import { TextInput as AstryxTextInput } from "@astryxdesign/core/TextInput";
import * as stylex from "@stylexjs/stylex";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  type InputHTMLAttributes,
  type MouseEventHandler,
  type ReactNode,
  use,
  useState,
  useSyncExternalStore,
} from "react";
import { useLinkState } from "../connection";
import { KeyHint } from "../ui/key-hint";

const styles = stylex.create({
  group: { display: "flex", flexDirection: "column", gap: 8 },
  groupHead: { display: "flex", minHeight: 28, alignItems: "center", gap: 8, paddingInline: 2 },
  groupTitle: { margin: 0, fontSize: 12.5, fontWeight: 600, color: "var(--color-text-secondary)" },
  count: { fontSize: 12, color: "var(--precision-text3)" },
  grow: { flex: 1 },
  card: {
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    backgroundColor: "var(--color-background-card)",
    boxShadow: "var(--precision-card-shadow)",
  },
  part: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    paddingInline: 16,
    paddingBlockStart: 12,
    paddingBlockEnd: 16,
    backgroundColor: "var(--precision-sunk)",
    borderBlockEndWidth: { default: 1, ":last-child": 0 },
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
  },
  foot: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    paddingInline: 10,
    paddingBlock: 8,
  },
  row: {
    minHeight: 52,
    gap: 16,
    borderBlockEndColor: "var(--precision-line2)",
    paddingInline: 16,
    paddingBlock: 10,
    outlineStyle: "none",
    backgroundColor: {
      default: "transparent",
      ":focus-within": "color-mix(in srgb, var(--color-overlay-hover) 40%, transparent)",
      ":focus": "var(--color-overlay-hover)",
    },
  },
  dim: { opacity: 0.6 },
  joined: { borderBlockEndWidth: 0 },
  title: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    fontWeight: 500,
    overflowWrap: "anywhere",
  },
  hint: {
    display: "block",
    fontSize: 12,
    lineHeight: 1.5,
    color: "var(--precision-text3)",
    overflowWrap: "anywhere",
  },
  // Capped, so a long confirm question wraps instead of squeezing the row's title.
  actions: {
    display: "flex",
    maxWidth: 440,
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: 8,
  },
  keyed: { display: "contents" },
  question: { maxWidth: 260, fontSize: 12, textAlign: "end", color: "var(--color-text-secondary)" },
  form: { display: "flex", alignItems: "center", gap: 8 },
  muted: { margin: 0, fontSize: 12, lineHeight: 1.5, color: "var(--precision-text3)" },
  // The input takes its font from these two, and has no styling of its own to pass.
  field: { "--text-body-size": "12px" },
  fieldMono: { "--text-body-size": "12px", "--font-family-body": "var(--font-family-code)" },
  chip: {
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    borderRadius: 999,
    paddingInline: 7,
    paddingBlock: 1,
    fontSize: 11,
    fontWeight: 600,
    lineHeight: 1.5,
    whiteSpace: "nowrap",
    backgroundColor: "var(--color-overlay-hover)",
    color: "var(--color-text-secondary)",
  },
  chipok: { backgroundColor: "var(--color-accent-muted)", color: "var(--color-text-accent)" },
  chipwarn: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
  chipbad: { backgroundColor: "var(--color-error-muted)", color: "var(--color-text-error)" },
  chipinfo: { backgroundColor: "var(--color-info-muted)", color: "var(--color-text-info)" },
  chipDot: { width: 6, height: 6, borderRadius: "50%", backgroundColor: "currentColor" },
  box: {
    display: "grid",
    width: 30,
    height: 30,
    flexShrink: 0,
    placeItems: "center",
    borderRadius: 8,
    backgroundColor: "var(--precision-sunk)",
    color: "var(--color-text-secondary)",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
  },
  boxLarge: { width: 40, height: 40, borderRadius: 10 },
  facts: {
    display: "grid",
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
  },
  fact: {
    display: "flex",
    minWidth: 0,
    flexDirection: "column",
    gap: 2,
    paddingBlock: 12,
    paddingInline: 16,
    borderInlineStartWidth: { default: 1, ":first-child": 0 },
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--precision-line2)",
  },
  factKey: { fontSize: 12, color: "var(--precision-text3)" },
  factValue: { overflow: "hidden", fontWeight: 600, textOverflow: "ellipsis", whiteSpace: "nowrap" },
  boxinfo: { backgroundColor: "var(--color-info-muted)", color: "var(--color-text-info)" },
  boxwarn: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
});

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

/** True directly in a Group's list, where everything has to be a list item. */
const InGroup = createContext(false);

export function Problem({ error }: { error: { message: string } | string | null | undefined }) {
  const inGroup = use(InGroup);
  if (!error) return null;
  const banner = <Banner status="error" role="alert" title={typeof error === "string" ? error : error.message} />;
  return inGroup ? <li {...stylex.props(styles.part)}>{banner}</li> : banner;
}

export function GroupTitle({ children }: { children: ReactNode }) {
  return (
    <Heading level={2} xstyle={styles.groupTitle}>
      {children}
    </Heading>
  );
}

/** A titled group, with its count and an action at the right. Without a title it is only the card. */
export function Group({
  title,
  count,
  action,
  children,
}: {
  title?: string;
  count?: number;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section {...stylex.props(styles.group)}>
      {title ? (
        <div {...stylex.props(styles.groupHead)}>
          <GroupTitle>{title}</GroupTitle>
          {count === undefined ? null : <span {...stylex.props(styles.count)}>{count}</span>}
          <span {...stylex.props(styles.grow)} />
          {action}
        </div>
      ) : null}
      <Section variant="transparent" padding={0} xstyle={styles.card}>
        <InGroup value={true}>
          <List hasDividers>{children}</List>
        </InGroup>
      </Section>
    </section>
  );
}

/** What is in a Group and is not a Row: a form, a notice, what an open Row shows. */
export function Part({ xstyle, children }: { xstyle?: stylex.StyleXStyles; children: ReactNode }) {
  return (
    <li {...stylex.props(styles.part, xstyle)}>
      <InGroup value={false}>{children}</InGroup>
    </li>
  );
}

function watchHints(changed: () => void) {
  const watcher = new MutationObserver(changed);
  watcher.observe(document.documentElement, { attributes: true, attributeFilter: ["data-hints"] });
  return () => watcher.disconnect();
}

const hintsOn = () => document.documentElement.hasAttribute("data-hints");

/** A key on a Settings control: the `chord` is shown when shortcut hints are on. */
export function Keyed({ action, chord, children }: { action: string; chord: string; children: ReactNode }) {
  const hints = useSyncExternalStore(watchHints, hintsOn);
  return (
    <span data-key={action} {...stylex.props(styles.keyed)}>
      {hints ? <KeyHint>{chord}</KeyHint> : null}
      {children}
    </span>
  );
}

/** Press what this key stands for: in the row that has the focus, otherwise the first on the page. */
export function pressKeyed(action: string): void {
  // A dialog on top has the keys; nothing behind it goes off.
  if (document.querySelector("dialog[open]")) return;
  const chosen = `[data-key="${action}"] :is(button:not(:disabled), input[role=switch]:not([aria-disabled=true]))`;
  const row = document.activeElement?.closest("[data-setting-row]");
  const control = row?.querySelector<HTMLElement>(chosen) ?? document.querySelector<HTMLElement>(chosen);
  control?.focus();
  control?.click();
}

/** Put the cursor in the field with this label. */
export function focusField(label: string): void {
  [...document.querySelectorAll("label")].find((l) => l.textContent?.includes(label))?.control?.focus();
}

export const HINTS_KEY = "always_show_shortcuts";

/** Walk the rows of a Settings page. */
export function stepRow(by: 1 | -1): void {
  if (document.querySelector("dialog[open]")) return;
  const rows = [...document.querySelectorAll<HTMLElement>("[data-setting-row]")];
  const at = rows.indexOf(document.activeElement?.closest("[data-setting-row]") as HTMLElement);
  (rows[at + by] ?? (at < 0 ? rows[0] : undefined))?.focus();
}

/** The last line of a card: actions that concern the whole list. */
export function Foot({ children }: { children: ReactNode }) {
  return (
    <li {...stylex.props(styles.foot)}>
      <InGroup value={false}>{children}</InGroup>
    </li>
  );
}

/** A strip of facts across a card: Version, Running since, Busy now. */
export function Facts({ facts }: { facts: { label: string; value: string }[] }) {
  return (
    <li {...stylex.props(styles.facts)}>
      {facts.map((f) => (
        <div key={f.label} {...stylex.props(styles.fact)}>
          <span {...stylex.props(styles.factKey)}>{f.label}</span>
          <span {...stylex.props(styles.factValue)}>{f.value}</span>
        </div>
      ))}
    </li>
  );
}

export type Tone = "ok" | "warn" | "bad" | "info" | "plain";

const CHIP_TONES = {
  ok: styles.chipok,
  warn: styles.chipwarn,
  bad: styles.chipbad,
  info: styles.chipinfo,
  plain: null,
} as const;

/** A short state beside a row's title: "Linked", "This device", "Needs login". */
export function Chip({ tone = "plain", dot = false, children }: { tone?: Tone; dot?: boolean; children: string }) {
  return (
    <span {...stylex.props(styles.chip, CHIP_TONES[tone])}>
      {dot ? <span {...stylex.props(styles.chipDot)} /> : null}
      {children}
    </span>
  );
}

const BOX_TONES = { info: styles.boxinfo, warn: styles.boxwarn, plain: null } as const;

/** The square at the start of a row, around an icon or two letters. */
export function IconBox({
  large = false,
  tone = "plain",
  children,
}: {
  large?: boolean;
  tone?: keyof typeof BOX_TONES;
  children: ReactNode;
}) {
  return (
    <span aria-hidden="true" {...stylex.props(styles.box, large && styles.boxLarge, BOX_TONES[tone])}>
      {children}
    </span>
  );
}

export function Row({
  title,
  hint,
  start,
  joined = false,
  dim = false,
  children,
}: {
  title: ReactNode;
  hint?: ReactNode;
  /** An icon box or a mark before the title. */
  start?: ReactNode;
  /** What this row opens follows right under it, without a line between them. */
  joined?: boolean;
  /** A row that no longer counts, like a disabled board member. */
  dim?: boolean;
  children?: ReactNode;
}) {
  return (
    <ListItem
      data-setting-row
      tabIndex={-1}
      xstyle={[styles.row, joined && styles.joined, dim && styles.dim]}
      {...(start ? { startContent: start } : {})}
      label={<span {...stylex.props(styles.title)}>{title}</span>}
      description={hint ? <span {...stylex.props(styles.hint)}>{hint}</span> : undefined}
      endContent={
        children ? (
          <InGroup value={false}>
            <div {...stylex.props(styles.actions)}>{children}</div>
          </InGroup>
        ) : undefined
      }
    />
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
    <AstryxSwitch
      label={label}
      isLabelHidden
      size="sm"
      value={on}
      isDisabled={Boolean(disabled)}
      onChange={(next) => onChange(next)}
    />
  );
}

const VARIANTS = { primary: "primary", secondary: "secondary", danger: "destructive", ghost: "ghost" } as const;

export function SmallButton({
  kind = "secondary",
  type = "button",
  disabled,
  title,
  onClick,
  children,
}: {
  kind?: keyof typeof VARIANTS;
  type?: "button" | "submit";
  disabled?: boolean;
  title?: string | undefined;
  onClick?: MouseEventHandler<HTMLElement>;
  children: string;
}) {
  return (
    <Button
      size="sm"
      variant={VARIANTS[kind]}
      type={type}
      label={children}
      isDisabled={Boolean(disabled)}
      {...(title ? { tooltip: title } : {})}
      {...(onClick ? { onClick } : {})}
    />
  );
}

/** Hard to undo: asks in place, so a stray click does nothing. */
export function ConfirmButton({
  label,
  question,
  onConfirm,
  disabled,
  kind = "danger",
}: {
  label: string;
  question: string;
  onConfirm: () => void;
  disabled?: boolean;
  /** How the first button looks; the confirming one is always the danger kind. */
  kind?: keyof typeof VARIANTS;
}) {
  const [asking, setAsking] = useState(false);
  if (!asking) {
    return (
      <SmallButton kind={kind} disabled={disabled ?? false} onClick={() => setAsking(true)}>
        {label}
      </SmallButton>
    );
  }
  return (
    <>
      <span {...stylex.props(styles.question)}>{question}</span>
      <SmallButton
        kind="danger"
        onClick={() => {
          setAsking(false);
          onConfirm();
        }}
      >
        {`Yes, ${label.toLowerCase()}`}
      </SmallButton>
      <SmallButton onClick={() => setAsking(false)}>Cancel</SmallButton>
    </>
  );
}

export function TextInput({
  label,
  showLabel = false,
  width,
  mono = false,
  type = "text",
  value,
  onChange,
  autoFocus = false,
  disabled = false,
  placeholder,
  autoComplete,
  onKeyDown,
  ...native
}: {
  label: string;
  showLabel?: boolean;
  width?: number;
  mono?: boolean;
  value: string;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "width" | "size" | "className" | "style">) {
  return (
    <AstryxTextInput
      {...(native as object)}
      label={label}
      isLabelHidden={!showLabel}
      size="sm"
      // Astryx types only the text kinds; the load limits stay number fields with their min and step.
      type={type as "text"}
      value={value}
      hasAutoFocus={autoFocus}
      isDisabled={disabled}
      controlClassName={stylex.props(mono ? styles.fieldMono : styles.field).className ?? ""}
      onChange={(_, event) => onChange?.(event)}
      {...(width === undefined ? {} : { width })}
      {...(placeholder === undefined ? {} : { placeholder })}
      {...(autoComplete === undefined ? {} : { autoComplete })}
      {...(onKeyDown === undefined ? {} : { onKeyDown })}
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
      {...stylex.props(styles.form)}
      onSubmit={(event) => {
        event.preventDefault();
        if (!changed) return;
        onSave(shown.trim());
        setDraft(null);
      }}
    >
      <TextInput label={label} value={shown} onChange={(e) => setDraft(e.target.value)} width={220} />
      {changed ? (
        <SmallButton kind="primary" type="submit" disabled={busy}>
          Save
        </SmallButton>
      ) : null}
    </form>
  );
}

/** A page on the web, in the system's browser; only http(s), whatever the server sent. */
export function openWeb(url: string): void {
  if (/^https?:\/\//i.test(url)) window.open(url, "_blank", "noreferrer");
}

export function Muted({ children }: { children: ReactNode }) {
  return <p {...stylex.props(styles.muted)}>{children}</p>;
}

export function when(ms: number | null | undefined): string {
  return ms ? new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : "Never";
}

/** "25 Nov": a day without the time, as Settings lists when something began. */
export function day(ms: number | null | undefined): string {
  return ms ? new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short" }) : "Never";
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
