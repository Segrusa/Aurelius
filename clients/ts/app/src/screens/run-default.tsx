// The Model, Effort and Mode chips of every composer and property (OPR-5 point 5): one label for
// "no choice" ("Your default · Opus 5.5", "Kim's default · …"), the menu starts with "Use your
// default" and ends with "Change my defaults". An old server keeps the labels it had.

import { expectBody } from "@aurelius/client";
import { useNavigate } from "@tanstack/react-router";
import type { ReactElement } from "react";
import { useConnection, useLinkState } from "../connection";
import { useTicket } from "../data";
import { Chip } from "../ui/chip";
import { Menu } from "../ui/menu";
import { effortName, MODE_LABELS } from "./employee-logic";
import {
  defaultLabel,
  MY_DEFAULTS_KEY,
  RUN_DEFAULTS_OF_FEATURE,
  type RunLogin,
  resetLabel,
  resolvedDefaults,
  USER_RUN_DEFAULTS_FEATURE,
  whoseDefault,
} from "./run-default-logic";
import { modelName, useSetting } from "./settings-kit";

/** Whose login a ticket's work runs on; none without a ticket (then the reader's own). */
export function useTicketLogin(ticketId: string | null | undefined): RunLogin | null {
  const detail = useTicket(ticketId ?? null);
  const t = detail.data?.ticket;
  return t ? { id: t.login_user, username: t.login_username } : null;
}

/** What the chips say without a choice for this run. */
export interface RunDefaultLabels {
  model: string;
  effort: string;
  mode: string;
  /** The menu's first entry: "Use your default". */
  reset: string;
}

/**
 * The labels of "no choice" for work that runs on `login` (none: the reader's own session). Null on
 * an old server (no `user_run_defaults`): the caller keeps its old label there.
 */
export function useRunDefaults(login?: RunLogin | null): RunDefaultLabels | null {
  const connection = useConnection();
  const { features, user } = useLinkState();
  const has = features.includes(USER_RUN_DEFAULTS_FEATURE);
  const other = login && user && login.id !== user.id ? login : null;
  // Another board member's value needs a newer server; without it the label is just "Kim's default".
  const readOther = other !== null && features.includes(RUN_DEFAULTS_OF_FEATURE);
  const view = useSetting(readOther ? `user_run_defaults_of:${other.id}` : "user_run_defaults", async () => {
    if (!has || (other && !readOther)) return null;
    const reply = other
      ? await connection.call("user.run_defaults_of", { user_id: other.id })
      : await connection.call("user.run_defaults");
    return expectBody(reply, "user_run_defaults").value;
  });
  if (!has) return null;
  const whose = whoseDefault(login, user?.id);
  const values = resolvedDefaults(
    view.data,
    modelName,
    effortName,
    (m) => MODE_LABELS[m as keyof typeof MODE_LABELS] ?? m,
  );
  const known = view.data !== null && view.data !== undefined;
  return {
    model: defaultLabel(whose, values.model),
    effort: defaultLabel(whose, values.effort),
    mode: defaultLabel(whose, known ? values.mode : null),
    reset: resetLabel(whose),
  };
}

/**
 * A run chip: the chosen value, else `fallback`. With `labels` (a new server) the menu starts with
 * "Use your default" and ends with "Change my defaults"; without, the "default" entry is `fallback`.
 */
export function RunMenu({
  label,
  value,
  options,
  fallback,
  labels,
  onPick,
  quiet = false,
  open,
  onOpenChange,
  trigger,
}: {
  label: "Model" | "Effort" | "Mode";
  /** The choice for this run; null or "default": none. */
  value: string | null | undefined;
  /** The values to choose, without "default". */
  options: { key: string; label: string }[];
  fallback: string;
  labels: RunDefaultLabels | null;
  onPick: (key: string) => void;
  quiet?: boolean;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Another trigger than the named chip (it gets the chip text). */
  trigger?: (text: string) => ReactElement;
}) {
  const navigate = useNavigate();
  const chosen = value && value !== "default" ? value : null;
  const text = chosen ? (options.find((o) => o.key === chosen)?.label ?? chosen) : fallback;
  const items = [
    { key: "default", label: labels ? labels.reset : fallback },
    ...options.filter((o) => o.key !== "default"),
    ...(labels ? [{ key: MY_DEFAULTS_KEY, label: "Change my defaults", dividerBefore: true }] : []),
  ];
  return (
    <Menu
      label={label}
      value={chosen ?? "default"}
      options={items}
      {...(open === undefined ? {} : { open })}
      {...(onOpenChange ? { onOpenChange } : {})}
      onPick={(key) => {
        if (key === MY_DEFAULTS_KEY) void navigate({ to: "/settings/$section", params: { section: "run-defaults" } });
        else onPick(key);
      }}
      trigger={
        trigger ? (
          trigger(text)
        ) : (
          <Chip name={label} quiet={quiet}>
            {text}
          </Chip>
        )
      }
    />
  );
}
