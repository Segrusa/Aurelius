// Adapters from the old overlays to Astryx, for screens not migrated yet. Goes when nothing imports it.

import { Dialog as AstryxDialog } from "@astryxdesign/core/Dialog";
import { Tooltip as AstryxTooltip } from "@astryxdesign/core/Tooltip";
import { cloneElement, createContext, type ReactElement, type ReactNode, use } from "react";

export { Menu } from "./menu";

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

export const TooltipProvider = ({ children }: { children: ReactNode }) => children;

export function Tooltip({ text, children }: { text: string; children: ReactElement }) {
  return (
    <AstryxTooltip content={text} delay={400} hasHoverIndication={false}>
      {children}
    </AstryxTooltip>
  );
}
