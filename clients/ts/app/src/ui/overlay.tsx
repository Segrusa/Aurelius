// Dialog, menu and tooltip on Astryx, with what the app's keys need around them.

import { Dialog as AstryxDialog } from "@astryxdesign/core/Dialog";
import { DropdownMenu } from "@astryxdesign/core/DropdownMenu";
import { Icon } from "@astryxdesign/core/Icon";
import { Tooltip as AstryxTooltip } from "@astryxdesign/core/Tooltip";
import {
  cloneElement,
  createContext,
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
  use,
  useContext,
} from "react";
import { KeysContext } from "../keys/provider";

const CloseContext = createContext<() => void>(() => {});

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
    <AstryxDialog
      isOpen={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      aria-label={title}
      width={width}
      maxHeight="calc(100vh - 48px)"
      padding={0}
      purpose="form"
      {...(top ? { position: { top: "14vh" } } : {})}
    >
      <CloseContext value={onClose}>{children}</CloseContext>
    </AstryxDialog>
  );
}

export function DialogClose({ children }: { asChild?: boolean; children: ReactElement<{ onClick?: () => void }> }) {
  const close = use(CloseContext);
  return cloneElement(children, { onClick: close });
}

export function Menu<T extends string>({
  trigger,
  options,
  value,
  onPick,
  label,
  open,
  onOpenChange,
}: {
  trigger: ReactElement;
  options: { key: T; label: string; danger?: boolean }[];
  value?: T;
  onPick: (key: T) => void;
  label: string;
  /** Only for a menu a key also opens. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const keys = useContext(KeysContext);
  const moveWithJK = (event: KeyboardEvent) => {
    if (event.key !== "j" && event.key !== "k") return;
    const menu = (event.target as Element).closest("[role=menu]");
    if (!menu) return;
    event.preventDefault();
    const items = [...menu.querySelectorAll<HTMLElement>("[role=menuitem]")];
    const at = items.indexOf(document.activeElement as HTMLElement);
    items[at + (event.key === "j" ? 1 : -1)]?.focus();
  };
  return (
    <span style={{ display: "contents" }} onKeyDown={moveWithJK}>
      <DropdownMenu
        menuMaxHeight={300}
        {...(open === undefined ? {} : { isMenuOpen: open })}
        {...(onOpenChange ? { onOpenChange } : {})}
        renderTrigger={(triggerProps) =>
          cloneElement(trigger as ReactElement<Record<string, unknown>>, {
            ...triggerProps,
            "aria-label": label,
            onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
              // After a pick the focus is back here, and Ctrl Enter means the form, not this menu.
              if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                event.preventDefault();
                keys?.press(event.nativeEvent);
                return;
              }
              triggerProps.onKeyDown(event);
            },
          })
        }
        items={options.map((o) => ({
          label: o.label,
          onClick: () => onPick(o.key),
          ...(o.danger ? { variant: "destructive" as const } : {}),
          ...(o.key === value ? { endContent: <Icon icon="check" size="sm" color="accent" /> } : {}),
        }))}
      />
    </span>
  );
}

export const TooltipProvider = ({ children }: { children: ReactNode }) => children;

export function Tooltip({ text, children }: { text: string; children: ReactElement }) {
  return (
    <AstryxTooltip content={text} delay={400} hasHoverIndication={false}>
      {children}
    </AstryxTooltip>
  );
}
