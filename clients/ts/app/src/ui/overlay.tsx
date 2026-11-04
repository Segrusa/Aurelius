// Dialog, menu and tooltip: the behaviour (focus, keyboard, screen readers) from Radix, the look from
// the design. Opening takes 160 ms, in CSS; closing is instant.

import * as RadixDialog from "@radix-ui/react-dialog";
import * as RadixMenu from "@radix-ui/react-dropdown-menu";
import * as RadixTooltip from "@radix-ui/react-tooltip";
import type { ReactElement, ReactNode } from "react";

export function Dialog({
  open,
  onClose,
  title,
  children,
  width = 640,
  top = false,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  width?: number;
  top?: boolean;
}) {
  return (
    <RadixDialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="overlay-fade fixed inset-0 z-40 bg-black/50" />
        <RadixDialog.Content
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            const field = (event.currentTarget as HTMLElement).querySelector<HTMLElement>("[data-autofocus]");
            if (field) {
              event.preventDefault();
              field.focus();
            }
          }}
          className={`dialog-rise fixed left-1/2 z-40 flex max-h-[calc(100vh-48px)] w-[calc(100vw-32px)] -translate-x-1/2 flex-col overflow-hidden rounded-[12px] border border-line bg-pop text-text shadow-pop ${top ? "top-[14vh]" : "top-1/2 -translate-y-1/2"}`}
          style={{ maxWidth: width }}
        >
          <RadixDialog.Title className="sr-only">{title}</RadixDialog.Title>
          {children}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}

export const DialogClose = RadixDialog.Close;

export function Menu<T extends string>({
  trigger,
  options,
  value,
  onPick,
  label,
}: {
  trigger: ReactElement;
  options: { key: T; label: string; danger?: boolean }[];
  value?: T;
  onPick: (key: T) => void;
  label: string;
}) {
  return (
    <RadixMenu.Root modal={false}>
      <RadixMenu.Trigger asChild aria-label={label}>
        {trigger}
      </RadixMenu.Trigger>
      <RadixMenu.Portal>
        <RadixMenu.Content
          align="start"
          sideOffset={4}
          className="menu-rise z-50 max-h-[300px] min-w-[160px] overflow-y-auto rounded-[8px] border border-line bg-pop p-1 text-text shadow-pop"
        >
          {options.map((o) => (
            <RadixMenu.Item
              key={o.key}
              onSelect={() => onPick(o.key)}
              className={`flex h-7 cursor-pointer items-center rounded-[5px] px-2 text-[13px] outline-none data-[highlighted]:bg-sel ${o.danger ? "text-red " : ""}${o.key === value ? "font-medium text-acc-text" : ""}`}
            >
              {o.label}
            </RadixMenu.Item>
          ))}
        </RadixMenu.Content>
      </RadixMenu.Portal>
    </RadixMenu.Root>
  );
}

export const TooltipProvider = ({ children }: { children: ReactNode }) => (
  <RadixTooltip.Provider delayDuration={400} skipDelayDuration={200}>
    {children}
  </RadixTooltip.Provider>
);

export function Tooltip({ text, children }: { text: string; children: ReactElement }) {
  return (
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          sideOffset={6}
          className="menu-rise z-50 rounded-[5px] border border-line bg-pop px-2 py-1 text-[12px] text-text2 shadow-pop"
        >
          {text}
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
}
