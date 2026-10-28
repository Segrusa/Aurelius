//! ```mermaid fences drawn as diagrams in markdown views. A fence that is still open (a streaming
//! answer) or that doesn't parse keeps the normal code block; copy and selection keep the source.

use std::sync::Arc;

use gpui_kit::component::text::{markdown_ast::Node, MarkdownNode, MarkdownParseContext, MarkdownPlugin};
use gpui_kit::*;
use mermaid_rs_renderer as mmdr;

use crate::theme;

const NAME: &str = "mermaid";
/// The SVG is rasterised at its own size, so it is drawn at twice the size and shown at half for sharp HiDPI text.
const SCALE: f32 = 2.;

/// Add with `TextView::plugin(Mermaid)`.
pub struct Mermaid;

struct Diagram {
    light: Arc<Image>,
    dark: Arc<Image>,
    width: f32,
    height: f32,
}

impl MarkdownPlugin for Mermaid {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        NAME
    }

    // Parsers run off the UI thread and only again when the text changes, so both themes are drawn here once.
    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Code(code) = node else { return None };
        if !code.lang.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(NAME)) {
            return None;
        }
        let source = cx.node_source(node)?;
        if !closed(source) {
            return None;
        }
        let (light, width, height) = svg(&code.value, false)?;
        let (dark, ..) = svg(&code.value, true)?;
        let image = |svg: String| Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()));
        Some(
            MarkdownNode::new(NAME, Diagram { light: image(light), dark: image(dark), width, height })
                .text(code.value.clone())
                .markdown(source)
                .accessibility_label("Mermaid diagram"),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        let d = node.data::<Diagram>().expect("mermaid node data");
        let image = if theme::is_light() { &d.light } else { &d.dark };
        // shortcut: in a pane narrower than the diagram it shrinks inside a fixed-height box (blank above and below); fine until wide diagrams are common.
        div().py_1().child(img(ImageSource::from(image.clone())).w(px(d.width)).max_w_full().h(px(d.height)).object_fit(ObjectFit::Contain))
    }
}

/// Whether a fenced block's source has its closing fence; while an answer streams it doesn't yet.
fn closed(source: &str) -> bool {
    let mut lines = source.trim_end().lines();
    let open = lines.next().unwrap_or("").trim_start();
    let fence: String = open.chars().take_while(|&c| c == '`' || c == '~').collect();
    let last = lines.last().map(str::trim);
    fence.len() >= 3 && last.is_some_and(|l| l.starts_with(&fence) && l.trim_start_matches(fence.chars().next().unwrap()).is_empty())
}

/// The diagram as SVG on a transparent background, with its size in pixels; `None` when it doesn't parse.
fn svg(code: &str, dark: bool) -> Option<(String, f32, f32)> {
    let parsed = mmdr::parse_mermaid_strict(code).ok()?;
    let mut theme = if dark { mmdr::Theme::dark() } else { mmdr::Theme::mermaid_default() };
    theme.background = "none".into();
    let config = mmdr::LayoutConfig::default();
    let layout = mmdr::compute_layout(&parsed.graph, &theme, &config);
    let size = mmdr::measure_svg_dimensions(&layout, &config, None);
    let (width, height) = (size.viewbox_width, size.viewbox_height);
    if !(width.is_finite() && height.is_finite() && width > 0. && height > 0.) {
        return None;
    }
    let svg = mmdr::render::render_svg_with_dimensions(&layout, &theme, &config, Some((width * SCALE, height * SCALE)));
    Some((svg, width, height))
}

#[cfg(test)]
mod tests {
    use super::{closed, svg};

    #[test]
    fn valid_diagram_draws_both_themes() {
        let (light, w, h) = svg("flowchart LR\n  A[Start] --> B{Ok?}\n  B -->|yes| C[Done]", false).expect("light");
        let (dark, ..) = svg("flowchart LR\n  A[Start] --> B{Ok?}\n  B -->|yes| C[Done]", true).expect("dark");
        assert!(w > 0. && h > 0.);
        assert!(light.starts_with("<svg") && dark.starts_with("<svg"));
        assert_ne!(light, dark, "themes differ");
        assert!(!light.contains("foreignObject"), "resvg can't draw HTML labels");
        assert!(svg("sequenceDiagram\n  Alice->>Bob: Hi", true).is_some());
    }

    #[test]
    fn invalid_diagram_falls_back() {
        assert!(svg("this is not mermaid", false).is_none());
        assert!(svg("", false).is_none());
    }

    #[test]
    fn open_fence_is_not_drawn() {
        assert!(closed("```mermaid\ngraph TD\nA-->B\n```"));
        assert!(closed("~~~~mermaid\nA-->B\n~~~~\n"));
        assert!(!closed("```mermaid\ngraph TD\nA-->B"));
        assert!(!closed("```mermaid\ngraph TD\nA-->B\n``"));
        assert!(!closed("```mermaid"));
    }
}
