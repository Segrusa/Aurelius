# New session — design spec (ticket #60)

Status: design, ready to build. Owner: Pixel (UI/UX). Client: desktop (`clients/ts/app`).
Reference: the Claude desktop "New session" screen (one calm page, a composer at the
bottom, the choices as chips under it). We take the pattern, not the look: Aurelius
tokens, components and words stay.

## 1. Goal

One general starting point where the board member picks a few things and starts work
by typing, without first deciding which dialog to open. Today the same act is split
over **New ticket**, **Give work** and the **Quick task** segment inside both. The New
session view brings them into one page; the dialogs stay for the shortcuts that open
them from elsewhere (palette, board "+", "New ticket instead").

No server or protocol change: every start is a `ticket.create` that the dialogs already
send (`ticketCreateParams`, `quickCreateParams`, the Give work body).

## 2. Where it lives

| Item | Spec |
|---|---|
| Route | `/new` (`NewSessionScreen`, `screens/new-session.tsx` + `new-session-logic.ts`) |
| Sidebar | First row of `NAV_MAIN`, above Inbox: label **New session**, icon = the existing `NewIcon`. The `+` next to Search in the sidebar head opens `/new` instead of the New ticket dialog. |
| Shortcut | `secondary-n` is already the page-dependent **New** (`NewItem`). Where it opens Give work today, it goes to `/new` with the composer focused instead; on pages with their own New (projects, memory, ideas…) it stays. No new chord. `Ctrl+T` stays New ticket (dialog). |
| Palette | Command "New session" (first in the Create group). |
| Start page | `/` keeps redirecting to `/inbox` (the inbox is where "needs you" lives). Setting in Settings → You: *Start on* Inbox / New session, default Inbox. Optional, may be a follow-up. |
| Title bar | Page title "New session". |

## 3. Layout (wireframe)

```
┌ sidebar ┐┌──────────────────────────────────────────────────────────────┐
│         ││ New session                                                   │
│ + New   ││                                                               │
│  session││                                                               │
│ Inbox   ││              What should happen next?            (h2, muted)  │
│ Board   ││                                                               │
│ …       ││        ┌ Project ─────────────────────────────┐               │
│         ││        │ [A] Aurelius                       ✓ │  ← list card  │
│         ││        │ [M] Munchy                           │    (max 6,    │
│         ││        │ [P] Planora                          │     then      │
│         ││        │ ──────────────────────────────────── │     "More…")  │
│         ││        │ All projects…                        │               │
│         ││        └──────────────────────────────────────┘               │
│         ││        ┌──────────────────────────────────────┐               │
│         ││        │ ↻ Continue: #58 Ticket history (Doing)│  ← max 3      │
│         ││        │ ↻ Continue: #57 …          (Review)  │    recent     │
│         ││        └──────────────────────────────────────┘               │
│         ││                                                               │
│         ││  ┌─────────────────────────────────────────────────────────┐  │
│         ││  │ +  Describe a ticket or give work…                   📎 │  │
│         ││  └─────────────────────────────────────────────────────────┘  │
│         ││   [A Aurelius ▾] [Ticket|Quick task] [Dispatcher ▾]           │
│         ││                         [Model ▾] [Effort ▾] [Mode ▾] ⏎ Start │
└─────────┘└──────────────────────────────────────────────────────────────┘
```

- Content column: max-width 760 px, centred. Composer pinned to the bottom of the page
  with 24 px bottom padding; the centre block (heading, project card, continue card) is
  vertically centred in the space above it, max-width 320 px like the reference.
- Below 720 px window width: the chip row wraps into two lines; the centre block keeps
  only the project card (Continue hides).
- Empty states: no project → centre block says "Create a project first: work always
  belongs to one." with a **New project** button; composer disabled. No active
  employee → the employee chip reads "No employee yet" and only Ticket (to the
  dispatcher) can start.

## 4. Components

### 4.1 Project card (centre)
- Same data as the Give work project picker (`useProjects`), workspace-filtered.
- Rows: `Mark` (initial, `markColor`) + name, check on the chosen one. Picking a row sets
  the project chip below; both show the same state.
- Default: the workspace project (`useWorkspace().id`), else the last used
  (`storage` key `new_session.project`), else the only project, else none (must pick).
- More than 6 projects: first 6 by last use, then **All projects…** opens the existing
  `Menu` with search.
- Role `listbox`, arrow keys move, Enter picks, focus returns to the composer.

### 4.2 Continue card (centre, optional)
- Up to 3 of the board member's tickets in Doing/Review/Testing, newest activity first
  (`my-tickets` data). Row: number, title (1 line, ellipsis), column as a muted pill.
- Click → `/board?ticket=<id>` (ticket panel). Hidden when there are none.

### 4.3 Composer
- `TextArea` without a visible label (aria-label "What needs doing"), 1 line, grows to
  10 lines, then scrolls. Placeholder follows the kind:
  - Ticket: "Describe a ticket or give work…"
  - Quick task: "What needs doing? Starts right away on its own branch"
- `+` on the left = attach (same `useAttachments` + `AttachRow` shown above the box when
  there are files; drag-and-drop and paste on the whole page).
- Title: none in the composer. Ticket title = `titleOfGoal(text)` (first line), as in
  Give work. Need a separate title or labels/deadline/priority → the **More options**
  link (see 4.5).
- Enter = newline, `Ctrl/Cmd+Enter` = Start (same chord as the dialogs, `KeyHint` on the
  button). Text survives navigation away and back (draft kept in `overlays`-like store
  per host, cleared on a successful start).

### 4.4 Chip row (under the composer)
Uses the existing `Chip` + `Menu` exactly like the dialogs. Left group = *what and
where*, right group = *how*.

| Chip | Options | Default | Shown when |
|---|---|---|---|
| Project | projects | see 4.1 | always |
| Kind | `SegmentedControl` Ticket / Quick task | Ticket (last used remembered) | `quickEnabled(features)` |
| Who | **Dispatcher** (Ticket only), then active employees with `description: title ?? role` | Ticket: Dispatcher. Quick: `quickAssignee(…, LAST_QUICK)` | always |
| Model | `useModels()` | Default | always |
| Effort | Default, low … max | Default | always |
| Mode | MODES, default label "Owner's default" | Owner's default | Ticket only |
| Start | primary button | — | always |

The right group sits right-aligned on the same line when it fits.

### 4.5 Footer line and More options
- One muted line above the chip row, same texts as the dialogs:
  - Ticket + Dispatcher: "The dispatcher picks the owner. You are responsible."
  - Ticket + employee: "<Name> reports back to you when it is done."
  - Quick: `QUICK_NOTE`.
  - Offline: `NoConnectionNote`.
- **More options** (text button at the end of that line, Ticket only) opens the New
  ticket dialog prefilled (`overlays.open("ticket", { projectId, title, description })`)
  for priority, labels, deadline, responsible, "Create without starting".

### 4.6 Start
| Kind / Who | Request | Toast | After |
|---|---|---|---|
| Ticket + Dispatcher | `ticket.create` start=true, owner none, responsible = me (if `ticket_responsible`) | "Ticket created and started." | open the new ticket's chat (`/chat?channel=<conversation>`), as New ticket and Give work do since #58; `/board?ticket=<new id>` without ticket channels or when the ticket cannot be read |
| Ticket + employee | Give work body (owner = employee, start=true, run mode/model/effort) | "Created and started with <Name>." | same |
| Quick task | `quickCreateParams` | "Quick task started with <Name>." | same |

- Use `useSend` (busy, error `Banner`, `Held` for secrets) unchanged.
- Remember project, kind and who (`new_session.*`, plus `LAST_QUICK` / `LAST_GIVEN`).
- Start is disabled until project + non-empty text (+ an employee for Quick).

## 5. Keyboard and accessibility
- Key context `NewSession`. `Ctrl+Enter` start; reuse the Give work cycle actions
  (`CycleGiveProject`, `CycleTaskManager`, `CycleTaskModel`, `CycleTaskEffort`,
  `CycleTaskMode`) with the same chords, bound in this context too.
- Tab order: project card → continue card → composer → chips left to right → Start.
- Every chip has its `name` (aria) as in the dialogs; the chosen value is the text.
- Errors in a `role="alert"` Banner above the composer; a toast confirms success.
- Contrast and focus rings from the theme; nothing new.

## 6. Out of scope (follow-ups)
- Mobile (a matching tab or `+` page) — separate ticket after desktop is accepted.
- "Import Claude Code CLI sessions" from the reference: no Aurelius counterpart.
- Variants (Known gap from ticket #16) could get a chip here later.
- Making New session the default start page is optional (§2).

## 7. Acceptance criteria
1. Sidebar shows **New session** first; `+` in the sidebar head and the general New (`secondary-n`, where it opened Give work) open it with the composer focused.
2. From that page alone a board member can start: a ticket for the dispatcher, a ticket for a chosen employee, and (with the feature) a quick task, choosing project, model, effort and (ticket) mode.
3. Each start sends the same request as the dialog it replaces and lands on the new ticket's chat, as the dialogs do.
4. Defaults follow §4.1/§4.4 and are remembered per host.
5. Empty, offline and error states show text and never an endless "Loading…".
6. Desktop type check, Biome and the app's unit tests pass; logic (defaults, request building, footer text) lives in `new-session-logic.ts` with tests.
