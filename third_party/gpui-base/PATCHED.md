# gpui-base 0.7.0, patched

Copy of [gpui-base 0.7.0](https://crates.io/crates/gpui-base/0.7.0) from
crates.io (Apache-2.0, https://github.com/longbridge/gpui-kit), used through
`[patch.crates-io]` in the workspace `Cargo.toml`. Only `Cargo.lock`,
`Cargo.toml.orig` and the registry marker files were left out.

## What changed and why

TextView tracks the link under the pointer (for the pointing-hand cursor) but
exposes nothing, so an app can't show the hovered URL in a status line the
way a browser does. The patch adds an app-wide hook:

```rust
gpui_base::text::on_link_hover(cx, |url: Option<SharedString>, window, cx| { ... });
```

- `src/text/text_view.rs`: `on_link_hover` stores the handler in a global
  (`LinkHover`) with the URL last reported; `report_link_hover` calls it only
  when the hovered link changes. A leave (`None`) only clears the link that
  text itself reported, so entering a link in one text and leaving one in
  another within the same mouse move ends on the new link.
- `src/text/inline.rs`: the existing mouse-move handler that repaints when
  the pointer enters, leaves or moves between links now also runs when the
  pointer is off the text (treated as "no link"), and reports the change.
  Cursor, selection, wrapping and click handling are untouched.

It is app-wide rather than a per-TextView builder because a per-view handler
would have to be threaded through `TextViewState`, the node context,
`InlineFlow` and gpui-component's compat `TextView`, a much bigger diff.

Not covered: links inside inline objects (`inline_object.rs`), which the app
doesn't render. The gap between the lines of a wrapped link reports a short
`None`; the app delays hiding to avoid flicker.

### Retained layouts of replaced paragraphs

`src/text/inline.rs` keeps each paragraph's shaped text in a thread-local
table (`RETAINED_LAYOUTS`) for the next frame. Entries of paragraph states
that were dropped (a re-parsed document) were only swept once the table held
4096 entries, so a long streamed reply left up to 4096 dead layouts (about
20 MB) behind for good. The sweep now runs whenever the table doubled since
the last sweep (at 256 entries at the latest), so dead entries never outnumber
the live ones by more than that minimum. Test:
`replaced_documents_do_not_pile_up_dead_layouts`.

## Updating

Drop this folder and the `[patch.crates-io]` entry once upstream has an
equivalent hook; otherwise re-apply `upstream.diff` on the new version.
