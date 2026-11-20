// What the Employees, Knowledge and Projects screens share: the page's head, its tabs, and the confirm.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Dialog } from "@astryxdesign/core/Dialog";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Layout, LayoutContent, LayoutFooter, LayoutHeader } from "@astryxdesign/core/Layout";
import { Selector } from "@astryxdesign/core/Selector";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { useRouterState } from "@tanstack/react-router";
import { type ComponentProps, type KeyboardEvent, type ReactNode, useContext } from "react";
import { type Feature, shown } from "../hidden";
import { KeysContext } from "../keys/provider";
import { DialogHead } from "../ui/dialog-head";
import { useDelayed } from "../ui/use-delayed";
import { topCentered } from "./dialog-logic";
import { InboxBell } from "./inbox-bell";

const styles = stylex.create({
  page: { display: "flex", minHeight: 0, flex: 1, flexDirection: "column" },
  head: {
    display: "flex",
    minHeight: 44,
    flexWrap: "wrap",
    alignItems: "center",
    gap: 10,
    paddingBlock: 6,
    paddingInlineStart: 18,
    paddingInlineEnd: 12,
  },
  title: { display: "flex", minWidth: 0, alignItems: "center", gap: 8, margin: 0, fontSize: 13, fontWeight: 500 },
  sub: { fontSize: 12, color: "var(--precision-text3)" },
  actions: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8, marginInlineStart: "auto" },
  eyebrow: {
    margin: 0,
    fontSize: 11.5,
    fontWeight: 600,
    letterSpacing: "0.04em",
    color: "var(--precision-text3)",
  },
  hint: { margin: 0, fontSize: 12, color: "var(--precision-text3)" },
  body: { display: "flex", flexDirection: "column", gap: 12 },
  footer: { display: "flex", justifyContent: "flex-end", gap: 8, paddingInline: 16, paddingBlock: 12 },
  confirmBody: { lineHeight: 1.5, color: "var(--color-text-secondary)" },
  loading: { display: "flex", flexDirection: "column", gap: 8, padding: 24 },
  contents: { display: "contents" },
});

/** A screen: a column that fills what the frame leaves and scrolls inside, never itself. */
export function Page({ children }: { children: ReactNode }) {
  return <div {...stylex.props(styles.page)}>{children}</div>;
}

export function PageHead({
  title,
  sub,
  tabs,
  children,
}: {
  title: ReactNode;
  sub?: ReactNode;
  tabs?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <LayoutHeader hasDivider padding={0} data-titlebar>
      <div {...stylex.props(styles.head)}>
        <Heading level={1} xstyle={styles.title}>
          {title}
        </Heading>
        {sub ? <span {...stylex.props(styles.sub)}>{sub}</span> : null}
        {tabs}
        <div {...stylex.props(styles.actions)}>
          {children}
          <InboxBell />
        </div>
      </div>
    </LayoutHeader>
  );
}

export function Eyebrow({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <Text as="p" display="block" xstyle={styles.eyebrow} {...(title ? { title } : {})}>
      {children}
    </Text>
  );
}

/** The small grey line under a field or a section. */
export function Hint({ children }: { children: ReactNode }) {
  return (
    <Text as="p" display="block" xstyle={styles.hint}>
      {children}
    </Text>
  );
}

export const Problem = ({ children }: { children: ReactNode }) => (
  <Banner status="error" role="alert" title={children} />
);

/** Shown only when the wait lasts: a fast answer never flashes it. */
export function Loading({ rows = 3 }: { rows?: number }) {
  const shown = useDelayed(true);
  if (!shown) return null;
  return (
    <div {...stylex.props(styles.loading)}>
      {Array.from({ length: rows }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: placeholders have nothing else to tell them apart.
        <Skeleton key={i} index={i} height={16} />
      ))}
    </div>
  );
}

const noNavigation = () => {};

const KNOWLEDGE: { key: string; label: string; feature?: Feature }[] = shown([
  { key: "guidelines", label: "Guidelines" },
  { key: "memory", label: "Memory" },
  { key: "ideas", label: "Ideas", feature: "Ideas" },
  { key: "vault", label: "Vault", feature: "Vault" },
]);

export function KnowledgeTabs() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const current =
    pathname === "/projects"
      ? "projects"
      : (KNOWLEDGE.find((t) => pathname.startsWith(`/knowledge/${t.key}`))?.key ?? "");
  return (
    <TabList aria-label="Knowledge" size="sm" value={current} onChange={noNavigation}>
      {KNOWLEDGE.map((t) => (
        <Tab key={t.key} value={t.key} label={t.label} href={`/knowledge/${t.key}`} />
      ))}
      <Tab value="projects" label="Projects" href="/projects" />
    </TabList>
  );
}

export function EmployeesTabs() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const current = pathname === "/employees" ? "list" : pathname.startsWith("/employees/org") ? "org" : "";
  return (
    <TabList aria-label="Employees" size="sm" value={current} onChange={noNavigation}>
      <Tab value="list" label="List" href="/employees" />
      <Tab value="org" label="Org chart" href="/employees/org" />
      <Tab value="guidelines" label="Guidelines" href="/knowledge/guidelines" />
    </TabList>
  );
}

/** Every dialog of these screens: a title, what it asks, and its buttons at the bottom. */
export function FormDialog({
  title,
  start,
  width = 520,
  top = false,
  footer,
  onClose,
  children,
}: {
  title: string;
  start?: ReactNode;
  width?: number;
  top?: boolean;
  footer: ReactNode;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <Dialog
      isOpen
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      width={width}
      maxHeight="calc(100vh - 48px)"
      purpose="form"
      padding={0}
      {...(top ? { position: topCentered(width) } : {})}
    >
      <Layout
        header={<DialogHead title={title} {...(start ? { startContent: start } : {})} />}
        content={
          <LayoutContent padding={4}>
            <div {...stylex.props(styles.body)}>{children}</div>
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <div {...stylex.props(styles.footer)}>{footer}</div>
          </LayoutFooter>
        }
      />
    </Dialog>
  );
}

/** A Selector inside a dialog, where Ctrl Enter on its trigger means the form, not this control. */
export function DialogSelector(props: ComponentProps<typeof Selector>) {
  const keys = useContext(KeysContext);
  const passOn = (event: KeyboardEvent<HTMLSpanElement>) => {
    if (event.key !== "Enter" || !(event.ctrlKey || event.metaKey)) return;
    if ((event.target as Element).getAttribute("aria-expanded") === "true") return;
    event.preventDefault();
    event.stopPropagation();
    keys?.press(event.nativeEvent);
  };
  return (
    <span {...stylex.props(styles.contents)} onKeyDownCapture={passOn}>
      <Selector width="100%" {...props} />
    </span>
  );
}

export function Confirm({
  title,
  body,
  action,
  danger = false,
  busy = false,
  onConfirm,
  onClose,
}: {
  title: string;
  body: ReactNode;
  action: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <FormDialog
      title={title}
      width={460}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button
            data-autofocus
            label={action}
            variant={danger ? "destructive" : "primary"}
            isDisabled={busy}
            onClick={onConfirm}
          />
        </>
      }
    >
      <div {...stylex.props(styles.confirmBody)}>{body}</div>
    </FormDialog>
  );
}

/** "Sun 1 Nov", or with the year when it is not this one. */
export function shortDay(ms: number): string {
  const d = new Date(ms);
  const weekday = d.toLocaleDateString("en-GB", { weekday: "short" });
  const month = d.toLocaleDateString("en-GB", { month: "short" });
  const year = d.getFullYear() === new Date().getFullYear() ? "" : ` ${d.getFullYear()}`;
  return `${weekday} ${d.getDate()} ${month}${year}`;
}

export const hhmm = (ms: number) => new Date(ms).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

export function Empty({ title, description, children }: { title: string; description?: string; children?: ReactNode }) {
  return (
    <EmptyState
      title={title}
      headingLevel={2}
      {...(description ? { description } : {})}
      {...(children ? { actions: children } : {})}
    />
  );
}

export const failureOf = (e: unknown) => (e instanceof Error ? e.message : "This did not work.");
