# Plan panel — UI design

Status: design, ready to build. Owner: Pixel (UI/UX). Client: desktop (`clients/ts/app`).
Mockup: `mockup.html` in this folder (open it in a browser; the bar at the top switches state).
Words, tokens and components stay the ones the app has; nothing here needs a server or
protocol change.

## 1. Problem

A plan (`claude.ExitPlanMode`, `update === "plan"`) shows today as a card in the feed
(`UpdateCard` in `chat-message.tsx`) with the `PlanCard` approval under it
(`approval-card.tsx:327`). A real plan is long: the card pushes the conversation away,
the decision buttons sit far below the first line, and commenting on a passage means
scrolling between the text and the comment box. Once the feed moves on, the plan scrolls
out of view while the agent works from it.

## 2. Principle

A plan opens in **the same panel at the right as a design** (`artifact-dock.tsx`). The feed
keeps a short card that says *that* there is a plan and what it is about; the panel is
where it is *read* and *decided*. One panel at a time: opening a plan replaces a design and
the other way round, exactly as two designs do now.

| Part | Today | New |
|---|---|---|
| Feed | full plan text + decision card | compact card: label, title, size, **Open plan** |
| Panel | — | plan text, inline comments, decision footer |
| Decision | buttons under the text in the feed | footer of the panel, always visible |

## 3. Feed card (compact)

Same `UpdateCard` look (label row, urgent border while pending).

```
PLAN · WAITING FOR YOU   Titan                       [Open plan]
Add a resizable plan panel to the conversation
6 steps · 4 files                               ← one muted line
```

- Title: `planTitle(m.text)` (exists). Size line: number of top-level list items under the
  first "steps"/"plan" heading, else the number of `##` sections; files = distinct paths in
  backticks. Pure function `planSummary(text)` in `chat-logic.ts`, with tests.
- **Open plan** (`Button`, primary while pending, secondary after) opens the panel. When this
  plan is already open the button reads **Showing** and is disabled.
- After a decision the label turns into the decision (`PLAN · APPROVED`, `PLAN · CHANGES
  REQUESTED`, `PLAN · REJECTED`) and the existing decision card (`DecisionCard`) stays under it.
- The inline fold (`▸ Show plan`) goes. Fallback, see §8.

## 4. Panel anatomy

Width, resize handle, min 360 px, max 70 %, remembered width: **reuse the dock's**
(`ResizeHandle`, `DOCK_*` in `artifact-viewer.tsx`). Same `autoSaveId`, so a plan and a
design open at the same width.

```
┌ Head 44px ─────────────────────────────────────────────┐
│ Add a resizable plan panel…       [Waiting for you]   ✕│
├ Meta 32px ─────────────────────────────────────────────┤
│ Titan · 14:32 · 6 steps · 4 files                       │
├ Body (scrolls) ────────────────────────────────────────┤
│  Markdown, max 72ch, 20px side padding                  │
│  highlighted quote ¹  …                                 │
│        ┌ floating on selection ┐                        │
│        │ 💬 Comment            │                        │
├ Comments (only when ≥1) ───────────────────────────────┤
│ ¹ “keep the dock width” — Also for plans, yes.       ✕ │
├ Footer ────────────────────────────────────────────────┤
│ [What should change?                               ]    │
│ [Approve + Auto] [Approve, edits] [Request changes] [Reject]│
└─────────────────────────────────────────────────────────┘
```

### 4.1 Head
- `Heading level 2` with the plan title, ellipsis (same `dockTitle` style).
- Status chip: `Waiting for you` (warning), `Approved · auto` / `Approved · edits` (success),
  `Changes requested` (info), `Rejected` (error). Text, not colour alone.
- Close (`IconButton` ghost, "Close plan").

### 4.2 Meta row
Author · time · size (same `planSummary`).

### 4.3 Body
- `Markdown` (existing component), `max-width: 72ch`, padding 20 px, line-height 1.6.
- Headings `##` get `scroll-margin-top` so the outline can jump to them (Could, §9).
- **Commenting moves into the text.** Selecting text shows a small floating `Button`
  "Comment" just above the selection (not a bar below the plan). Click → an inline
  `TextInput` card right under the paragraph, `Enter` adds, `Esc` cancels.
- An added comment marks its quote in the text: background `--color-warning-muted`, dotted
  underline and a superscript number (¹ ²). Clicking the mark scrolls to its comment in the
  list and the other way round. Logic and feedback text stay `composePlanFeedback`.
- Only while pending: after a decision the text is read-only (no Comment button).

### 4.4 Comments strip
Only when there is at least one comment; max 30 % of the panel height, scrolls itself.
Each row: number, quote (muted, one line, ellipsis), comment, remove `✕`. Heading:
"Your comments".

### 4.5 Footer (sticky)
- While pending: `TextInput` "What should change?" (grows to 4 lines) + `ButtonGroup`
  "Decide on the plan" with the four existing buttons, same labels, same order, same
  toasts. Under 480 px panel width the group wraps to two rows: primary pair, then
  `Request changes` + `Reject`.
- The "Say what should change, then ask for changes." problem text stays.
- After a decision: the footer becomes one line, e.g. "You approved this plan at 14:40 ·
  auto mode", with the sent feedback foldable under it. No buttons.

## 5. When the panel opens

| Event | Panel |
|---|---|
| New plan arrives **waiting for you**, panel empty | opens it (no focus move, polite live region: "Plan ready for review") |
| New plan waiting, panel shows a design or another plan | does **not** take it over; the feed card is urgent and the head of the open panel gets a small pill "New plan waiting" that opens it |
| Board member clicks **Open plan** | opens, focus to the panel heading |
| `Alt+Shift+P` (existing `TogglePlanFold`) | opens / closes the latest plan; relabel to "Open / close the latest plan" |
| `Esc` in the panel (no input focused) | closes it, focus back to the card's Open plan |
| Thread opens | closes the dock, as today (`chat.tsx:1044`) |

Rationale for not stealing: a design being written takes the panel because the board
member just asked for it; a plan can arrive while they are reading something else.

## 6. No versions

A plan has no versions. Each plan message stands on its own: its card in the feed, its own
status and decision. When the agent sends a new plan after **Request changes**, that is
simply a new plan with a new card; the panel shows the plan whose card was opened. There
is no version picker, no "revised" label and no diff between plans.

## 7. Keyboard and accessibility
- `<aside aria-label="Plan: <title>">`; the head is a landmark heading.
- Decision keys (`Decision` key context: `x` / `Ctrl+Backspace` reject) apply when focus is
  in the panel, not only in the card. They must not fire while the text input has focus.
- Floating Comment button is reachable without a mouse: with a text selection made by
  keyboard (`Shift+arrows`), `Ctrl+Alt+M` opens the comment box (new action
  `CommentOnSelection`, context `PlanPanel`).
- Comment marks: not colour only (underline + number). Contrast of the mark ≥ 3:1 against
  the panel background in both themes.
- Status changes are announced in a polite live region.
- Hit targets ≥ 28 px (sm buttons), focus ring as everywhere (`outline 2px accent`).

## 8. Where the panel cannot be shown
- Window too narrow for feed (min 420 px) + panel (min 360 px): **Open plan** opens the
  plan in a `FormDialog` (width 880) with the same body and footer, as a design does when
  it cannot dock.
- Web client: a plan is Markdown, no sandbox, so unlike `DockedArtifact` it must **not**
  depend on `usePlatform().artifacts`. The plan dock works on the web too.
- Inbox (`employee-inbox.tsx`) keeps its card, but its "Open" goes to the conversation with
  the plan panel open (`?plan=<messageId>` search param).
- Mobile: out of this design (follow-up: plan as a full-screen page with a sticky decision
  bar, same structure).

## 9. Build notes (for Flux)
- `ArtifactTarget` gets `{ kind: "plan"; messageId: string }`; `targetKey` → `plan:<id>`.
  Rename the panel concept to "dock" where it says design only (`aria-label` stays
  specific: "Resize plan panel").
- New `screens/plan-dock.tsx` (`PlanDock`) using the dock's shell; move the comment state
  and `decide` out of `PlanCard` into a hook `usePlanDecision(approval)` so card fallback
  (§8 dialog) and dock share it.
- Pure logic with tests: `planSummary` and `shouldAutoOpenPlan(dockTarget, plan)`.
- Could (not in the first build): an outline popover from the `##` headings in the meta
  row; "Copy plan" in a More menu.

## 10. Acceptance
1. A pending plan shows a compact card in the feed and opens in the right panel; the
   decision buttons are visible without scrolling at any panel height ≥ 400 px.
2. The four decisions send exactly what they send today (`approval.decide_plan`, same
   feedback text from `composePlanFeedback`).
3. Comments are made from a selection in the panel and marked in the text.
4. A plan never replaces an open design by itself (§5).
5. Narrow window and web work as in §8.
6. Keyboard: Alt+Shift+P, Esc, x and comment-on-selection work; axe shows no new issues.

Pixel — UI/UX
