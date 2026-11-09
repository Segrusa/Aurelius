import type { ActivityLogEntry, EmployeeQueueItem } from "@aurelius/protocol";

export type ActivityCategory = "delivery" | "queue" | "run";

/** What the Activity log page narrows on; each is in its URL. */
export interface ActivityFilters {
  employee?: string;
  thread?: string;
  run?: string;
  ticket?: number;
  category?: ActivityCategory;
}

export const ACTIVITY_CATEGORIES: { value: ActivityCategory; label: string }[] = [
  { value: "delivery", label: "Delivery" },
  { value: "queue", label: "Queue" },
  { value: "run", label: "Run" },
];

const isCategory = (value: unknown): value is ActivityCategory => ACTIVITY_CATEGORIES.some((c) => c.value === value);

const text = (value: unknown): string | undefined => (typeof value === "string" && value !== "" ? value : undefined);

/** The URL's search, whatever someone typed there. */
export function activityFilters(search: Record<string, unknown>): ActivityFilters {
  const ticket = Number(search.ticket);
  const employee = text(search.employee);
  const thread = text(search.thread);
  const run = text(search.run);
  return {
    ...(employee ? { employee } : {}),
    ...(thread ? { thread } : {}),
    ...(run ? { run } : {}),
    ...(Number.isInteger(ticket) && ticket > 0 ? { ticket } : {}),
    ...(isCategory(search.category) ? { category: search.category } : {}),
  };
}

const RUN_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const THREAD_KEY = /^(ticket|channel|chat):\S+$/;
const TICKET = /^#?(\d{1,9})$/;

/** What the search box asks for: a ticket number, a thread key or a run id. Null: none of those. */
export function parseActivitySearch(query: string): Pick<ActivityFilters, "ticket" | "thread" | "run"> | null {
  const q = query.trim();
  if (q === "") return {};
  const ticket = TICKET.exec(q);
  if (ticket) return { ticket: Number(ticket[1]) };
  if (THREAD_KEY.test(q)) return { thread: q };
  if (RUN_ID.test(q)) return { run: q.toLowerCase() };
  return null;
}

const STEPS: Record<string, string> = {
  "delivery.delivered": "Delivered",
  "delivery.held": "Handoff held",
  "delivery.released": "Handoff delivered",
  "delivery.not_delivered": "Handoff not delivered",
  "delivery.scheduled": "Scheduled",
  "delivery.scheduled_delivered": "Scheduled message sent",
  "delivery.scheduled_cancelled": "Scheduled message cancelled",
  "delivery.scheduled_not_delivered": "Scheduled message not sent",
  "queue.claimed": "Picked up",
  "queue.cancelled": "Taken out of the queue",
  "queue.moved_up": "Moved to the front",
  "queue.held_back": "Held back",
  "queue.deferred": "Deferred",
  "queue.due_again": "Due again",
  "queue.parked": "Waits for an answer",
  "queue.unparked": "Goes on",
  "run.started": "Run started",
  "run.tool": "Tool call",
  "run.tool_failed": "Tool call failed",
  "run.waiting": "Run waits",
  "run.completed": "Run finished",
  "run.failed": "Run failed",
  "run.cancelled": "Run stopped",
  "run.approval_requested": "Approval asked",
  "run.approval_approved": "Approved",
  "run.approval_rejected": "Rejected",
  "run.approval_answered": "Answered",
  "run.approval_expired": "Approval expired",
  "run.approval_invalidated": "Approval dropped",
};

/** A step of a newer server shows as it is, without the underscores. */
export const stepLabel = (e: Pick<ActivityLogEntry, "category" | "step">): string =>
  STEPS[`${e.category}.${e.step}`] ?? e.step.replaceAll("_", " ");

export const categoryLabel = (category: string): string =>
  ACTIVITY_CATEGORIES.find((c) => c.value === category)?.label ?? category;

/** The server's own words, but a usage limit by its name. */
export const detailText = (detail: string | undefined | null): string =>
  detail === "quota" ? "usage limit" : (detail ?? "");

/** The end of a run id: enough to tell runs apart in a table. */
export const shortRun = (id: string): string => id.slice(-6);

const KINDS: Record<string, string> = {
  message: "Message",
  task: "Task",
  resume: "Answer",
  scheduled: "Scheduled message",
  handoff: "Handoff",
};

const SOURCES: Record<string, string> = { board: "from a board member", employee: "from a colleague" };

export function queueWhat(item: Pick<EmployeeQueueItem, "kind" | "source">): string {
  const source = SOURCES[item.source];
  const kind = KINDS[item.kind] ?? item.kind;
  return source ? `${kind} ${source}` : kind;
}

/** Where an item stands. `next`: the first in line; `time` writes a moment. */
export function queueState(
  item: Pick<EmployeeQueueItem, "state" | "waits_for" | "not_before_ms">,
  next: boolean,
  time: (ms: number) => string,
): string {
  if (item.state === "claimed") return "Working on it";
  switch (item.waits_for) {
    case "handoff":
      return "Held until the sender has caught up";
    case "question":
      return "Waits for an answer in this thread";
    case "limit":
      return `Usage limit, until ${time(item.not_before_ms)}`;
    case "later":
      return `Not before ${time(item.not_before_ms)}`;
    case "machine":
      return "Waits for the machine";
    default:
      return next ? "Next" : "In line";
  }
}

/** The first item that can be next: not one held back by something else. */
export const firstInLine = (items: Pick<EmployeeQueueItem, "state" | "waits_for" | "id">[]): string | null =>
  items.find((i) => i.state === "queued" && !i.waits_for)?.id ?? null;
