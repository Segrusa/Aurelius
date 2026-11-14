// The app's menu: Astryx's DropdownMenu plus j/k inside it, and Ctrl+Enter on its trigger meaning the form.

import { DropdownMenu } from "@astryxdesign/core/DropdownMenu";
import { Icon } from "@astryxdesign/core/Icon";
import { cloneElement, type KeyboardEvent, type ReactElement, type ReactNode, useContext } from "react";
import { KeysContext } from "../keys/provider";

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
  options: {
    key: T;
    label: string;
    /** A second, quieter line under the label. */
    description?: string;
    danger?: boolean;
    icon?: ReactNode;
    dividerBefore?: boolean;
  }[];
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
    // biome-ignore lint/a11y/noStaticElementInteractions: it only catches keys bubbling out of the menu.
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
        items={options.flatMap((o) => [
          ...(o.dividerBefore ? [{ type: "divider" as const }] : []),
          {
            label: o.label,
            ...(o.description ? { description: o.description } : {}),
            onClick: () => onPick(o.key),
            ...(o.icon ? { icon: o.icon } : {}),
            ...(o.danger ? { variant: "destructive" as const } : {}),
            ...(o.key === value ? { endContent: <Icon icon="check" size="sm" color="accent" /> } : {}),
          },
        ])}
      />
    </span>
  );
}
